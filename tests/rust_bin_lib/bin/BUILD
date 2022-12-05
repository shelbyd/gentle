import "^core/rust" as rust;

target "main" = rust::new_binary(#{
  main: "src/main.rs",

  crates: #{
    "lib": "//lib:lib",
    "left_pad": rust::crates_io("left_pad", "1.0.1", #{
      "sha256": "97235f118becc143458c5f59c34a66dea2cc303f14348c5c7f3072bbf786e62b",
    }),
  },
});
