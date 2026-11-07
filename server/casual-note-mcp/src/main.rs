//! The `casual-note-mcp` binary: MCP over stdio. See the library docs.

use std::path::PathBuf;
use std::process::ExitCode;

use casual_note_mcp::notebooks::Notebooks;
use casual_note_mcp::protocol::{Server, serve};
use casual_note_mcp::sandbox::Sandbox;

const USAGE: &str = "\
casual-note-mcp: an MCP server (stdio) for CasualNote notebooks

USAGE:
    casual-note-mcp --root <DIR> [--root <DIR>]... [--read-only]

OPTIONS:
    --root <DIR>    A directory the tools may read and write under. Repeatable.
                    Without one, every tool refuses.
    --read-only     Offer and allow reads only. Notebook files are never written.
    -h, --help      Print this help.
    -V, --version   Print the version.

A notebook is a file named *.casualnote, holding the bytes of a workspace export.

This connects your notes to another program and its AI. A note can contain
instructions aimed at that AI. If it follows them, it can read everything under
the folders you list here and send it out with that program's own tools, such as
web access or email. --read-only stops writes and does not stop this. List only
the notebooks this program needs, each with its own --root.

This server has no approval step: unless --read-only is set, writes commit when
the client calls them. Any approval is the client's.

The server reads JSON-RPC 2.0 messages, one per line, on stdin and writes replies
on stdout. Diagnostics go to stderr.";

fn main() -> ExitCode {
    let mut roots = Vec::new();
    let mut read_only = false;
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("-h" | "--help") => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            Some("-V" | "--version") => {
                println!("casual-note-mcp {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            Some("--read-only") => read_only = true,
            Some("--root") => match args.next() {
                Some(dir) => roots.push(PathBuf::from(dir)),
                None => return usage_error("--root needs a directory"),
            },
            _ => return usage_error(&format!("unexpected argument {}", arg.to_string_lossy())),
        }
    }
    let sandbox = match Sandbox::new(&roots) {
        Ok(s) => s,
        Err(e) => return usage_error(&e),
    };
    if sandbox.roots().is_empty() {
        eprintln!("casual-note-mcp: no --root given; every tool will refuse");
    }
    let mut server = Server::new(Notebooks::new(sandbox, read_only));
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    match serve(&mut server, &mut stdin.lock(), &mut stdout.lock()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("casual-note-mcp: {e}");
            ExitCode::FAILURE
        }
    }
}

fn usage_error(message: &str) -> ExitCode {
    eprintln!("casual-note-mcp: {message}\n\n{USAGE}");
    ExitCode::from(2)
}
