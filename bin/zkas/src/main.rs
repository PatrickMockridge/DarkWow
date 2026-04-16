/* This file is part of DarkFi (https://dark.fi)
 *
 * Copyright (C) 2020-2026 Dyne.org foundation
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU Affero General Public License as
 * published by the Free Software Foundation, either version 3 of the
 * License, or (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU Affero General Public License for more details.
 *
 * You should have received a copy of the GNU Affero General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 */

use std::{
    fs::{read_to_string, File},
    io::Write,
    path::Path,
    process::ExitCode,
};

use arg::Args;

use darkfi::{
    zkas::{Analyzer, Compiler, Lexer, Parser, ZkBinary},
    ANSI_LOGO,
};

const ABOUT: &str =
    concat!("zkas ", env!("CARGO_PKG_VERSION"), '\n', env!("CARGO_PKG_DESCRIPTION"));

const USAGE: &str = r#"
Usage: zkas [OPTIONS] <INPUT>

Arguments:
  <INPUT>    ZK script to compile

Options:
  -o <FILE>  Place the output into <FILE>
  -s         Strip debug symbols
  -p         Preprocess only; do not compile
  -i         Interactive semantic analysis
  -e         Examine decoded bytecode
  --version  Print version and exit
  -h         Print this help

Subcommands:
  validate   Validate a .zk.bin file
  rebuild    Rebuild .zk.bin files from .zk source files
"#;

const VALIDATE_USAGE: &str = r#"
Usage: zkas validate <BINARY>

Validate a ZK binary file (.zk.bin)

Arguments:
  <BINARY>   Path to the .zk.bin file to validate

Example:
  zkas validate src/contract/dao_escrow/proof/pay_premium_v1.zk.bin
"#;

const REBUILD_USAGE: &str = r#"
Usage: zkas rebuild <DIRECTORY>

Rebuild ZK binaries from source files in a directory

Arguments:
  <DIRECTORY>   Path to directory containing .zk source files

Example:
  zkas rebuild src/contract/dao_escrow/proof/
"#;

fn usage() {
    print!("{ANSI_LOGO}{ABOUT}\n{USAGE}");
}

fn validate_usage() {
    eprint!("{VALIDATE_USAGE}");
}

fn rebuild_usage() {
    eprint!("{REBUILD_USAGE}");
}

/// Validate a single ZK binary file
fn validate_binary(path: &Path) -> ExitCode {
    let bincode = match std::fs::read(path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed reading from \"{}\". {e}", path.display());
            return ExitCode::FAILURE
        }
    };

    match ZkBinary::decode(&bincode, false) {
        Ok(_) => {
            println!("OK: {}", path.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("CORRUPTED: {}: {}", path.display(), e);
            ExitCode::FAILURE
        }
    }
}

/// Rebuild all ZK binaries in a directory from .zk source files
fn rebuild_directory(path: &Path) -> ExitCode {
    if !path.is_dir() {
        eprintln!("Error: {} is not a directory", path.display());
        return ExitCode::FAILURE
    }

    let entries = match std::fs::read_dir(path) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed to read directory {}: {}", path.display(), e);
            return ExitCode::FAILURE
        }
    };

    let mut rebuilt = 0;
    let mut failed = 0;

    for entry in entries.flatten() {
        let source_path = entry.path();
        if source_path.extension().map_or(false, |e| e == "zk") {
            let bin_path = source_path.with_extension("zk.bin");

            // Read source
            let source = match read_to_string(&source_path) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Failed reading from \"{}\". {e}", source_path.display());
                    failed += 1;
                    continue
                }
            };

            // Clean up tabs, and convert CRLF to LF.
            let source = source.replace('\t', "    ").replace("\r\n", "\n");

            // Lex
            let lexer = Lexer::new(source_path.to_str().unwrap_or(""), source.chars());
            let tokens = match lexer.lex() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Lexer failed for \"{}\": {:?}", source_path.display(), e);
                    failed += 1;
                    continue
                }
            };

            // Parse
            let parser = Parser::new(source_path.to_str().unwrap_or(""), source.chars(), tokens);
            let (namespace, k, constants, witnesses, statements) = match parser.parse() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Parser failed for \"{}\": {:?}", source_path.display(), e);
                    failed += 1;
                    continue
                }
            };

            // Analyze
            let mut analyzer =
                Analyzer::new(source_path.to_str().unwrap_or(""), source.chars(), constants, witnesses, statements);
            if analyzer.analyze_types().is_err() {
                failed += 1;
                continue
            }

            // Compute source hash
            let source_hash = blake3::hash(source.as_bytes()).to_hex().to_string();

            // Compile
            let compiler = Compiler::new(
                source_path.to_str().unwrap_or(""),
                source.chars(),
                namespace,
                k,
                analyzer.constants,
                analyzer.witnesses,
                analyzer.statements,
                analyzer.literals,
                true, // include debug symbols
                Some(source_hash),
            );

            let bincode = match compiler.compile() {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Compiler failed for \"{}\": {:?}", source_path.display(), e);
                    failed += 1;
                    continue
                }
            };

            // Write output
            let mut file = match File::create(&bin_path) {
                Ok(v) => v,
                Err(e) => {
                    eprintln!("Error: Failed to create \"{}\": {}", bin_path.display(), e);
                    failed += 1;
                    continue
                }
            };

            if let Err(e) = file.write_all(&bincode) {
                eprintln!("Error: Failed to write to \"{}\": {}", bin_path.display(), e);
                failed += 1;
                continue
            }

            println!("Rebuilt: {}", bin_path.display());
            rebuilt += 1;
        }
    }

    println!("\nRebuilt {} binaries, {} failed", rebuilt, failed);

    if failed > 0 {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    // Check for --version flag BEFORE any argument parsing
    // This must be done from raw env args to avoid Args library parsing issues
    let args_vec: Vec<String> = std::env::args().collect();
    if args_vec.len() == 2 && args_vec[1] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS
    }

    // Check for subcommands
    if args_vec.len() >= 2 {
        match args_vec[1].as_str() {
            "validate" => {
                if args_vec.len() != 3 {
                    validate_usage();
                    return ExitCode::FAILURE
                }
                let path = Path::new(&args_vec[2]);
                return validate_binary(path)
            }
            "rebuild" => {
                if args_vec.len() != 3 {
                    rebuild_usage();
                    return ExitCode::FAILURE
                }
                let path = Path::new(&args_vec[2]);
                return rebuild_directory(path)
            }
            _ => {}
        }
    }

    let argv;
    let mut pflag = false;
    let mut iflag = false;
    let mut eflag = false;
    let mut sflag = false;
    let mut hflag = false;
    let mut vflag = false;
    let mut output = String::new();

    {
        let mut args = Args::new().with_cb(|args, flag| match flag {
            'p' => pflag = true,
            'i' => iflag = true,
            'e' => eflag = true,
            's' => sflag = true,
            'v' => vflag = true,
            'o' => output = args.eargf().to_string(),
            _ => hflag = true,
        });

        argv = args.parse();
    }

    // Version flag just prints the version hash and exits
    if vflag {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS
    }

    if hflag || argv.is_empty() {
        usage();
        return ExitCode::FAILURE
    }

    let filename = argv[0].as_str();
    let source = match read_to_string(filename) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed reading from \"{filename}\". {e}");
            return ExitCode::FAILURE
        }
    };

    // Clean up tabs, and convert CRLF to LF.
    let source = source.replace('\t', "    ").replace("\r\n", "\n");

    // ANCHOR: zkas
    // The lexer goes over the input file and separates its content into
    // tokens that get fed into a parser.
    let lexer = Lexer::new(filename, source.chars());
    let tokens = match lexer.lex() {
        Ok(v) => v,
        Err(_) => return ExitCode::FAILURE,
    };

    // The parser goes over the tokens provided by the lexer and builds
    // the initial AST, not caring much about the semantics, just enforcing
    // syntax and general structure.
    let parser = Parser::new(filename, source.chars(), tokens);
    let (namespace, k, constants, witnesses, statements) = match parser.parse() {
        Ok(v) => v,
        Err(_) => return ExitCode::FAILURE,
    };

    // The analyzer goes through the initial AST provided by the parser and
    // converts return and variable types to their correct forms, and also
    // checks that the semantics of the ZK script are correct.
    let mut analyzer = Analyzer::new(filename, source.chars(), constants, witnesses, statements);
    if analyzer.analyze_types().is_err() {
        return ExitCode::FAILURE
    }

    if iflag && analyzer.analyze_semantic().is_err() {
        return ExitCode::FAILURE
    }

    if pflag {
        println!("{:#?}", analyzer.constants);
        println!("{:#?}", analyzer.witnesses);
        println!("{:#?}", analyzer.statements);
        println!("{:#?}", analyzer.heap);
        return ExitCode::SUCCESS
    }

    // Compute source hash for binary verification
    let source_hash = blake3::hash(source.as_bytes()).to_hex().to_string();

    let compiler = Compiler::new(
        filename,
        source.chars(),
        namespace,
        k,
        analyzer.constants,
        analyzer.witnesses,
        analyzer.statements,
        analyzer.literals,
        !sflag,
        Some(source_hash),
    );

    let bincode = match compiler.compile() {
        Ok(v) => v,
        Err(_) => return ExitCode::FAILURE,
    };
    // ANCHOR_END: zkas

    let output = if output.is_empty() { format!("{filename}.bin") } else { output };

    let mut file = match File::create(&output) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: Failed to create \"{output}\". {e}");
            return ExitCode::FAILURE
        }
    };

    if let Err(e) = file.write_all(&bincode) {
        eprintln!("Error: Failed to write bincode to \"{output}\". {e}");
        return ExitCode::FAILURE
    };

    println!("Wrote output to {}", &output);

    if eflag {
        let zkbin = ZkBinary::decode(&bincode, true).unwrap();
        println!("{zkbin:#?}");
    }

    ExitCode::SUCCESS
}
