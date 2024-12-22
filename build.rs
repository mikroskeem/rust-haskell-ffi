use std::env;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::Deserialize;

type StdError = Box<dyn std::error::Error + Send + Sync>;

#[derive(Clone, Debug, Deserialize)]
struct CabalInstallPlan {
    r#type: String,

    id: String,

    //#[serde(rename = "pkg-name")]
    //pkg_name: String,

    //#[serde(rename = "pkg-version")]
    //pkg_version: String,
    #[serde(rename = "dist-dir")]
    dist_dir: Option<PathBuf>,
    // TODO: Should use this
    //depends: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct CabalPlan {
    #[serde(rename = "compiler-id")]
    compiler_id: String,

    #[serde(rename = "install-plan")]
    install_plan: Vec<CabalInstallPlan>,
}

#[derive(Debug)]
struct LoadedCabalPlan {
    project_id: String,
    compiler_id: String,
    dist_dir: PathBuf,
    dependencies: Vec<CabalInstallPlan>,
}

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

fn load_cabal_plan<P: AsRef<Path>>(output_dir: P) -> Result<LoadedCabalPlan, StdError> {
    let plan_file = File::open(output_dir.as_ref().join("cache/plan.json"))?;
    let reader = BufReader::new(plan_file);

    let plan: CabalPlan = serde_json::from_reader(reader)?;

    let configured_plan = plan
        .install_plan
        .iter()
        .find(|p| p.r#type == "configured")
        .expect("to have 'configured' install plan");
    let dist_dir = configured_plan
        .dist_dir
        .as_ref()
        .expect("to have 'dist-dir' in 'configured' install plan");

    let dependencies = plan
        .install_plan
        .iter()
        .filter(|p| p.r#type == "pre-existing")
        .cloned()
        .collect();

    Ok(LoadedCabalPlan {
        project_id: configured_plan.id.clone(),
        compiler_id: plan.compiler_id.clone(),
        dist_dir: dist_dir.to_path_buf(),
        dependencies,
    })
}

fn link_haskell_project<P: AsRef<Path>>(project_path: P, statically: bool) -> Result<(), StdError> {
    if statically {
        // TODO: libffi, libiconv, ghc-bignum needs libgmp etc.
        return Err(StdError::from("static haskell linking is currently broken"));
    }

    let project_path = project_path.as_ref();
    let link_type = if statically { "static" } else { "dylib" };

    // Load project details
    // TODO: make cabal output this
    let output_directory = project_path.join("dist-newstyle");

    // Load Cabal plan
    let plan = load_cabal_plan(&output_directory)?;
    let compiler_id = plan.compiler_id;
    let project_id = plan.project_id;
    let build_directory = plan.dist_dir.join("build");

    if !compiler_id.starts_with("ghc") {
        return Err(StdError::from(format!(
            "unsupported compiler '{compiler_id}'"
        )));
    }

    // Configure linker to look for Haskell runtime
    let ghc_version = cmd(&compiler_id, &["--numeric-version"])?;
    let ghc_libdir = PathBuf::from(cmd(&compiler_id, &["--print-libdir"])?);

    let lib_name = |name: &str| {
        if statically {
            format!("HS{name}")
        } else {
            format!("HS{name}-ghc{ghc_version}")
        }
    };

    for dependency in plan.dependencies {
        let dependency_dir = ghc_libdir.join(&dependency.id);
        let dependency_dir_str = dependency_dir.to_str().expect("dependency_dir to be a valid path");

        println!("cargo:rustc-link-search=native={dependency_dir_str}");
        println!(
            "cargo:rustc-link-lib={link_type}={}",
            lib_name(&dependency.id)
        );

        if !statically {
            println!("cargo:rustc-link-arg=-Wl,-rpath,{dependency_dir_str}");
        }
    }

    // TODO: nix-support/propagated-target-target-deps contains following libraries:
    if statically {
        println!("cargo:rustc-link-search=native=/nix/store/f9m8fq8d8cfx8dw4y5p6p34gqpgli96w-gmp-with-cxx-6.3.0/lib");
        println!("cargo:rustc-link-search=native=/nix/store/g5r20rs0qhcjcbf9dhbnbd9ksg0h0jmx-libiconv-50");
        println!("cargo:rustc-link-search=native=/nix/store/lb9b1yj8l0zqgj5f28m4k1lw9wcj8d1m-libffi-3.4.4");

        println!("cargo:rustc-link-lib={link_type}={}", "ffi");
        println!("cargo:rustc-link-lib={link_type}={}", "gmp");
        println!("cargo:rustc-link-lib={link_type}={}", "iconv");
    }

    // Configure linker to look for Haskell library
    let build_directory_str = build_directory
        .to_str()
        .expect("build directory to be valid string");
    println!("cargo:rustc-link-search=native={build_directory_str}");
    println!("cargo:rustc-link-lib={link_type}={}", lib_name(&project_id));
    if !statically {
        println!("cargo:rustc-link-arg=-Wl,-rpath,{build_directory_str}");
    }

    // Generate bindings
    let rts_include = ghc_libdir.join("rts/include");
    let rts_include_str = rts_include.to_str().unwrap();

    let header_path = build_directory.join("Safe_stub.h");
    let header_path_str = header_path.to_str().unwrap();

    let bindings = bindgen::Builder::default()
        .clang_arg(format!("-I{rts_include_str}"))
        .header(header_path_str)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .blocklist_function("^hs_")
        .generate()
        .expect("Unable to generate bindings");

    // Write the bindings to the $OUT_DIR/bindings.rs file.
    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("Couldn't write bindings!");

    Ok(())
}

fn main() -> Result<(), StdError> {
    link_haskell_project("./", false)?;

    Ok(())
}
