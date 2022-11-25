import "^core/rust" as rust;

task "main" = rust::binary(#{
  main: "src/main.rs",

  crates: #{
    "lib": "//lib:lib",
  },
});
