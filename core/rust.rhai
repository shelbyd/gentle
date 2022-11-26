fn binary(args) {
  let defaults = #{
    edition: "2021",
    crates: #{},
  };
  let args = defaults + args;

  gtl::exec("rustc", [
    "--color=always",
    `--edition=${args.edition}`,
    args.main,
  ]);
}
