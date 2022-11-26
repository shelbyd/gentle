fn binary(args) {
  let defaults = #{
    edition: "2021",
    crates: #{},
  };
  let args = defaults + args;

  let externs = [];
  for crate_name in args.crates.keys() {
    let to_build = args.crates[crate_name];
    let built = gtl::build(to_build);
    externs.push(`--extern=${crate_name}=${built.rlib}`);
  }

  let bin_out = `${gtl::out_dir}/${gtl::current_taskname}`;

  gtl::exec("rustc", externs + [
    "--color=always",
    `--edition=${args.edition}`,
    `-o${bin_out}`,
    args.main,
  ]);

  gtl::action_run(bin_out);
}

fn library(args) {
  let defaults = #{
    edition: "2021",
    crates: #{},
  };
  let args = defaults + args;

  let rlib_out = `${gtl::out_dir}/${gtl::current_taskname}.rlib`;

  gtl::exec("rustc", [
    "--color=always",
    "--crate-type=rlib",
    `-o${rlib_out}`,
    args.root,
  ]);

  #{
    rlib: rlib_out,
  }
}
