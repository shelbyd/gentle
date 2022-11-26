use include_dir::*;
use rhai::*;
use std::{path::PathBuf, process::Command as StdCommand};
use structopt::*;

mod target;

#[derive(StructOpt, Debug)]
struct Options {
    /// The root directory to work against. Will be inferred based on the current directory if not
    /// provided.
    #[structopt(long)]
    root: Option<PathBuf>,

    #[structopt(subcommand)]
    command: Command,
}

#[derive(StructOpt, Debug)]
enum Command {
    Run {
        // TODO(shelbyd): Support multiple targets.
        /// Target to run.
        target: target::Target,
    },
}

fn main() -> anyhow::Result<()> {
    let options = Options::from_args();

    let root = match options.root {
        Some(r) => r,
        None => std::env::current_dir()?,
    };

    match options.command {
        Command::Run { target } => {
            let package = target.package;
            let task = target.task;

            let dir = root.join(&package);
            let file = dir.join("BUILD");

            let mut engine = rhai::Engine::new();
            engine.set_module_resolver(GentleModuleResolver::default());
            engine.set_max_expr_depths(0, 0);

            {
                let task = task.clone();
                engine.register_custom_syntax(
                    ["task", "$expr$", "=", "$expr$"],
                    false,
                    move |context, inputs| {
                        let evaled = context.eval_expression_tree(&inputs[0])?;
                        let name: String = evaled.clone().try_cast().ok_or_else(|| {
                            EvalAltResult::ErrorMismatchDataType(
                                "String".to_string(),
                                evaled.type_name().to_string(),
                                inputs[0].position(),
                            )
                        })?;
                        if name != task {
                            return Ok(Dynamic::default());
                        }

                        Ok(context.eval_expression_tree(&inputs[1])?)
                    },
                )?;
            }

            let mut gtl_module = Module::new();
            gtl_module.set_native_fn(
                "exec",
                move |ctx: NativeCallContext<'_>, cmd: String, args: Array| {
                    dbg!(&cmd, &args);
                    let mut command = StdCommand::new(cmd);
                    command.current_dir(&dir);
                    for arg in args {
                        command.arg(arg.to_string());
                    }
                    let output = command.output().unwrap();
                    if output.status.success() {
                        Ok(String::from_utf8_lossy(&output.stdout).to_string())
                    } else {
                        Err(Box::new(EvalAltResult::ErrorRuntime(
                            Dynamic::from(String::from_utf8_lossy(&output.stderr).to_string()),
                            ctx.position(),
                        )))
                    }
                },
            );
            engine.register_static_module("gtl", gtl_module.into());

            engine.run_file(file)?;
            Ok(())
        }
    }
}

type RhaiResult<T> = Result<T, Box<EvalAltResult>>;

static CORE_DIR: Dir<'_> = include_dir!("$CARGO_MANIFEST_DIR/core");

struct GentleModuleResolver {
    file: rhai::module_resolvers::FileModuleResolver,
}

impl Default for GentleModuleResolver {
    fn default() -> Self {
        let file = rhai::module_resolvers::FileModuleResolver::new_with_extension("");
        GentleModuleResolver { file }
    }
}

impl rhai::ModuleResolver for GentleModuleResolver {
    fn resolve(
        &self,
        engine: &Engine,
        source: Option<&str>,
        path: &str,
        pos: Position,
    ) -> RhaiResult<Shared<Module>> {
        if let Some(core) = path.strip_prefix("^core/") {
            let contents = CORE_DIR
                .get_file(format!("{core}.rhai"))
                .ok_or(EvalAltResult::ErrorModuleNotFound(path.to_string(), pos))?
                .contents();
            let contents_str = String::from_utf8(contents.to_vec()).map_err(|utf8| {
                EvalAltResult::ErrorSystem(format!("Module '{path}' is not utf-8"), utf8.into())
            })?;
            let mut ast = engine.compile(contents_str).map_err(|e| {
                Box::new(EvalAltResult::ErrorInModule(
                    path.to_string(),
                    e.into(),
                    pos,
                ))
            })?;

            ast.set_source(path);

            let module = Module::eval_ast_as_new(Scope::new(), &ast, engine)
                .map_err(|e| Box::new(EvalAltResult::ErrorInModule(path.to_string(), e, pos)))?;
            return Ok(module.into());
        }

        self.file.resolve(engine, source, path, pos)
    }
}
