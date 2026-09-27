use std::process::{Command, Output};

const SEED: &str = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f";

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_piratepaperwallet"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn ironwood_default_and_legacy_sapling_are_selectable() {
    let output = run(&["--hdseed", SEED, "--nobip39"]);
    assert!(output.status.success());
    let json = String::from_utf8(output.stdout).unwrap();
    assert!(json.contains(
        "pirate1lmugzsm86svtmtuhj004f4ukhg7w0vqvjule5a42hgle3hdpc65fkmm5g9hkeuq6wthq69r3nzy"
    ));
    let output = run(&["--pool", "sapling", "--hdseed", SEED]);
    assert!(output.status.success());
    assert!(String::from_utf8(output.stdout)
        .unwrap()
        .contains("\"address\": \"zs1"));
}

#[test]
fn invalid_inputs_exit_unsuccessfully_without_spending_secrets() {
    for args in [
        vec!["--zaddrs", "-1"],
        vec!["--zaddrs", "0", "--hdseed", SEED],
        vec!["--vanity", "q", "--threads", "0"],
        vec!["--cointype", "2147483648", "--hdseed", SEED],
        vec!["--phrase", "not a valid phrase"],
        vec!["--partialphrase", "only two"],
        vec!["--format", "pdf", "--hdseed", SEED],
        vec!["--hdseed", "ff"],
    ] {
        let output = run(&args);
        assert!(!output.status.success(), "unexpected success for {args:?}");
        assert!(!String::from_utf8(output.stdout)
            .unwrap()
            .contains("private_key"));
    }
}

#[test]
fn refuses_to_overwrite_existing_wallet_output() {
    let path =
        std::env::temp_dir().join(format!("pirate-cli-existing-{}.json", std::process::id()));
    std::fs::write(&path, "existing wallet backup").unwrap();
    let output = run(&["--hdseed", SEED, "--output", path.to_str().unwrap()]);
    assert!(!output.status.success());
    assert_eq!(
        std::fs::read_to_string(&path).unwrap(),
        "existing wallet backup"
    );
    std::fs::remove_file(path).unwrap();
}
