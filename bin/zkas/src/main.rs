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
"#;

fn usage() {
    print!("{ANSI_LOGO}{ABOUT}\n{USAGE}");
}

fn main() -> ExitCode {
    // Check for --version flag BEFORE any argument parsing
    // This must be done from raw env args to avoid Args library parsing issues
    let args_vec: Vec<String> = std::env::args().collect();
    if args_vec.len() == 2 && args_vec[1] == "--version" {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return ExitCode::SUCCESS
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
