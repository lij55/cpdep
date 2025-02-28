use goblin::elf::{Elf};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::error::Error;
use regex::Regex;
use clap::{Parser};

fn find_library_path(lib_name: &str, mut user_path: Vec<&str>) -> Option<PathBuf> {
    // Check the system's library search paths (e.g., /lib, /usr/lib, etc.)
    let mut system_paths = vec!["/lib", "/usr/lib", "/lib64", "/usr/lib64", "/usr/local/lib"];
    if user_path.len() > 0 {
        system_paths.append(&mut user_path);
    }
    for path in system_paths {
        let lib_path = Path::new(path).join(lib_name);
        if lib_path.exists() {
            return Some(lib_path);
        }
    }

    // Check LD_LIBRARY_PATH environment variable
    if let Ok(ld_path) = env::var("LD_LIBRARY_PATH") {
        for path in ld_path.split(':') {
            let lib_path = Path::new(path).join(lib_name);
            if lib_path.exists() {
                return Some(lib_path);
            }
        }
    }

    None
}

fn extract_dependencies(elf_data: &[u8]) -> Vec<String> {
    let elf = Elf::parse(elf_data).expect("Failed to parse ELF file");
    elf.libraries.iter().map(|lib| lib.to_string()).collect()
}

fn resolve_dependencies_recursively(
    executable_path: &str,
    processed: &mut HashSet<String>,
) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    // Avoid processing the same library multiple times
    if processed.contains(executable_path) {
        return Ok(processed.clone());
    }

    let elf_data = fs::read(executable_path)?;
    let dependencies = extract_dependencies(&elf_data);

    for dep in dependencies {
        if !processed.contains(&dep) {
            processed.insert(dep.clone());
            // Recursively resolve dependencies of the found library
            resolve_dependencies_recursively(&dep, processed)?;
        }
    }

    Ok(processed.clone())
}

fn copy_libraries(libraries: &HashSet<String>, target_dir: &str, search_path: Vec<&str>) -> Result<(), Box<dyn std::error::Error>> {
    for lib in libraries {
        if let Some(dep_path) = find_library_path(lib, search_path.clone()) {

            // Copy the library to the target directory
            let target_path = Path::new(target_dir).join(dep_path.file_name().unwrap());
            println!("{} => {}", dep_path.display(), target_path.display());
            fs::copy(dep_path, target_path)?;

        } else {
            println!("Library {} not found", lib);
        }
    }

    Ok(())
}

fn resolve_dependencies(executable_path: &str) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let mut processed = HashSet::new();
    let mut all_dependencies = resolve_dependencies_recursively(executable_path, &mut processed)?;

    // Define the ignore list patterns
    let ignore_patterns = vec![
        Regex::new(r"ld-linux-x86-64.so.*").unwrap(),
        Regex::new(r"linux-vdso.so.*").unwrap(),
        Regex::new(r"libc.so.*").unwrap(),
    ];

    // Filter out dependencies that match the ignore list patterns
    all_dependencies.retain(|dep| {
        !ignore_patterns.iter().any(|regex| regex.is_match(dep))
    });
    //println!("{:?}", all_dependencies);
    Ok(all_dependencies)
}

// 命令行参数
#[derive(Parser, Debug)]
#[clap(name = "Dependency Resolver", version = "1.0", about = "Resolves and copies dependencies of a given executable")]
struct Args {
    /// Path to the executable (required)
    #[clap(required = true)]
    exe: String,

    /// Directory where dependencies will be copied (default is "output")
    #[clap(short, long, default_value = "output")]
    target: String,

    /// additional search path for lib
    #[clap(short, long)]
    search: Option<String>,

}

fn create_target_dirs(target_dir: &str) {

    // 如果目标目录不存在，创建它
    let target_path = Path::new(target_dir);
    if !target_path.exists() {
        if let Err(e) = fs::create_dir_all(target_path) {
            eprintln!("Error creating target directory '{}': {}", target_dir, e);
            std::process::exit(1);
        }
        println!("Created target directory: {}", target_dir);
    }

    // 计算目标路径
    //let bin_dir = Path::new(target_dir).join("bin");
    let lib_dir = Path::new(target_dir).join("libs");

    // // 创建 bin 目录
    // if !bin_dir.exists() {
    //     if let Err(e) = fs::create_dir_all(&bin_dir) {
    //         eprintln!("Error creating bin directory '{}': {}", bin_dir.display(), e);
    //         std::process::exit(1);
    //     }
    //     println!("Created bin directory: {}", bin_dir.display());
    // }

    // 创建 lib 目录
    if !lib_dir.exists() {
        if let Err(e) = fs::create_dir_all(&lib_dir) {
            eprintln!("Error creating lib directory '{}': {}", lib_dir.display(), e);
            std::process::exit(1);
        }
        println!("Created lib directory: {}", lib_dir.display());
    }

    // 创建 env.sh 文件并写入内容
    let env_sh_path = Path::new(target_dir).join("env.sh");
    let content = r#"SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
export PATH=${SCRIPT_DIR}:$PATH
export LD_LIBRARY_PATH=${SCRIPT_DIR}/libs
"#;
    if let Err(e) = fs::write(env_sh_path, content) {
        eprintln!("Error creating env.sh file: {}", e);
        std::process::exit(1);
    }
}

fn main() {
    let args = Args::parse();

    // 获取可执行文件路径
    let executable_path = &args.exe;

    // 获取目标目录，使用提供的或默认的目标目录 "output"
    let target_dir= &args.target;

    // target/exe, target/libs/, target/env.sh
    create_target_dirs(target_dir);

    let exe_filename = Path::new(&executable_path).file_name().unwrap().to_str().unwrap();

    fs::copy(executable_path, Path::new(target_dir)
        .join(exe_filename).to_str().unwrap()).expect("Failed to copy executable");

    let mut user_search_path = Vec::new();
    let search_path_str = args.search.unwrap();
    user_search_path.push(search_path_str.as_str());
    match resolve_dependencies(executable_path) {
        Ok(all_dependencies) => {
            copy_libraries(&all_dependencies,
                           Path::new(target_dir).join("libs").to_str().unwrap(),
                           user_search_path).expect("Failed to copy library");
        }
        Err(err) => {
            eprintln!("Error: {}", err);
        }
    }
}
