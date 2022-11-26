fn binary(args) {
  if gtl::current_action == "run" {
    let built = gtl::build(gtl::current_target);
    gtl::action_run(built.bin);
    return;
  }

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

  let common_args = externs + [
    "--color=always",
    `--edition=${args.edition}`,
  ];

  if gtl::current_action == "build" {
    let bin_out = `${gtl::out_dir}/${gtl::current_identifier}`;

    gtl::exec("rustc", common_args + [
      `-o${bin_out}`,
      args.main,
    ]);

    return #{
      bin: bin_out,
    };
  }

  if gtl::current_action == "test" {
    let bin_out = `${gtl::out_dir}/${gtl::current_identifier}-test`;

    gtl::exec("rustc", common_args + [
      `-o${bin_out}`,
      `--test`,
      args.main,
    ]);

    gtl::action_test(bin_out, ["--color=always"]);

    return;
  }

  throw `Unrecognized action ${gtl::current_action}`;
}

fn library(args) {
  let defaults = #{
    edition: "2021",
    crates: #{},
  };
  let args = defaults + args;

  let rlib_out = `${gtl::out_dir}/${gtl::current_identifier}.rlib`;

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
