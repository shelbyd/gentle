import "^core/rust" as rust;

target "main" = rust::binary(#{
  main: "src/main.rs",

  crates: #{
    "lib": "//lib:lib",
  },
});
