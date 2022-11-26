use include_dir::*;
use rhai::*;
use std::{path::PathBuf, process::Command as StdCommand};
use structopt::*;

mod target;
use target::*;

#[derive(StructOpt, Debug)]
struct Options {
    /// The root directory to work against. Will be inferred based on the current directory if not
    /// provided.
    #[structopt(long)]
    root: Option<PathBuf>,

    /// Action to perform.
    #[structopt(subcommand)]
    action: Action,
}

#[derive(StructOpt, Debug)]
enum Action {
    Run {
        /// Target to run.
        target: TargetAddress,
    },

    Test {
        /// Targets to run, supports matching.
        // TODO(shelbyd): Use TargetMatcher instead.
        target: TargetAddress,
    },
}

fn main() -> anyhow::Result<()> {
    let options = Options::from_args();

    let root = match options.root {
        Some(r) => r,
        None => std::env::current_dir()?,
    };

    match options.action {
        Action::Run { target } => {
            run_single_task(root, "run", target)?;
        }
        Action::Test { target } => {
            run_single_task(root, "test", target)?;
        }
    }

    Ok(())
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

fn run_single_task(root: PathBuf, action: &str, target: TargetAddress) -> RhaiResult<Dynamic> {
    let ident = target.identifier.clone();
    let file = root.join(&target.package).join("BUILD");

    let out_dir = "/home/shelby/.gentle";
    std::fs::create_dir_all(&out_dir).unwrap();

    let mut engine = rhai::Engine::new();
    engine.set_module_resolver(GentleModuleResolver::default());
    engine.set_max_expr_depths(0, 0);

    {
        let ident = ident.clone();
        engine.register_custom_syntax(
            ["target", "$expr$", "=", "$expr$"],
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
                if name != ident {
                    return Ok(Dynamic::default());
                }

                Err(Box::new(EvalAltResult::Return(
                    context.eval_expression_tree(&inputs[1])?,
                    inputs[1].position(),
                )))
            },
        )?;
    }

    let mut gtl_module = Module::new();
    {
        let file = file.clone();
        gtl_module.set_native_fn(
            "exec",
            move |ctx: NativeCallContext<'_>, cmd: &str, args: Array| {
                let mut command = StdCommand::new(cmd);
                command.current_dir(&file.parent().unwrap());
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
    }
    gtl_module.set_native_fn("action_run", |bin: &str| {
        let mut command = StdCommand::new(bin);
        let _ = command.status().unwrap();
        Ok(())
    });
    gtl_module.set_native_fn("build", move |task: &str| {
        let target: TargetAddress = task.parse().unwrap();
        run_single_task(root.clone(), "build", target)
    });
    gtl_module.set_var("out_dir", "/home/shelby/.gentle");
    gtl_module.set_var("current_identifier", ident);
    gtl_module.set_var("current_action", action.to_string());
    gtl_module.set_var("current_target", target.to_string());

    engine.register_static_module("gtl", gtl_module.into());

    engine.eval_file(file)
}
