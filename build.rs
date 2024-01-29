use std::process::{Command, Stdio};

type StdError = Box<dyn std::error::Error + Send + Sync>;

fn cmd(command: &str, args: &[&str]) -> Result<String, StdError> {
    let child = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .output()?;

    let stdout = String::from_utf8(child.stdout)?;
    let trimmed = stdout.trim();
    Ok(trimmed.to_string())
}

fn main() -> Result<(), StdError> {
    let ghc_version = cmd("ghc", &["--numeric-version"])?;
    let ghc_libdir = cmd("ghc", &["--print-libdir"])?;

    let project_name = "rust-haskell-ffi";
    let project_version = "0.1.0.0";

    // TODO: figure out how to get Cabal to output this
    let build_directory = format!("./dist-newstyle/build/aarch64-osx/ghc-{ghc_version}/{project_name}-{project_version}/build");

    println!("cargo:rustc-link-search=native={}/rts", ghc_libdir);
    println!("cargo:rustc-link-search=native={}", build_directory);

    println!(
        "cargo:rustc-link-lib={}",
        format!("HSrts-ghc{}", ghc_version)
    );
    println!(
        "cargo:rustc-link-lib={}",
        format!(
            "HS{}-{}-inplace-ghc{}",
            project_name, project_version, ghc_version
        )
    );

    Ok(())
}
