use goblin::elf::{Elf};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::collections::HashSet;
use std::error::Error;
use regex::Regex;
use clap::{Parser};
use std::process::{Command, Stdio};

fn get_ldd_dependencies(executable: &str) -> Vec<String> {
    let output = Command::new("ldd")
        .arg(executable)
        .stdout(Stdio::piped())
        .output()
        .expect("Failed to execute ldd");

    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut dependencies = Vec::new();

    // 创建正则表达式，用于移除路径后面的内存地址部分
    let re = Regex::new(r"\s+\(.*\)$").unwrap();

    for line in output_str.lines() {
        // 只处理格式类似 "libname => /path/to/lib" 或 "libname => not found" 的行
        if let Some(pos) = line.find("=>") {
            let path = &line[pos + 3..].trim(); // 获取 "=>" 后面的路径部分
            // 使用正则表达式移除内存地址部分
            let clean_path = re.replace_all(path, "");
            dependencies.push(clean_path.to_string());
        }
    }
    dependencies
}

fn copy_libraries(libraries: &HashSet<String>, target_dir: &str) -> Result<(), Box<dyn std::error::Error>> {
    for lib in libraries {
        let dep_path = Path::new(lib);

        // Copy the library to the target directory
        let target_path = Path::new(target_dir).join(dep_path.file_name().unwrap());
        println!("{} => {}", dep_path.display(), target_path.display());
        fs::copy(dep_path, target_path)?;

    }

    Ok(())
}

fn resolve_dependencies(executable_path: &str) -> Result<HashSet<String>, Box<dyn std::error::Error>> {
    let  all_dependencies = get_ldd_dependencies(executable_path);

    let mut result = all_dependencies.into_iter().collect::<HashSet<String>>();

    // Define the ignore list patterns
    let ignore_patterns = vec![
        Regex::new(r"ld-linux-x86-64.so.*").unwrap(),
        Regex::new(r"linux-vdso.so.*").unwrap(),
        Regex::new(r"libc.so.*").unwrap(),
    ];

    // Filter out dependencies that match the ignore list patterns
    result.retain(|dep| {
        !ignore_patterns.iter().any(|regex| regex.is_match(dep))
    });
    //println!("{:?}", all_dependencies);
    Ok(result)
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

    match resolve_dependencies(executable_path) {
        Ok(all_dependencies) => {
            copy_libraries(&all_dependencies,
                           Path::new(target_dir).join("libs").to_str().unwrap())
                .expect("Failed to copy library");
        }
        Err(err) => {
            eprintln!("Error: {}", err);
        }
    }
}
