//! Imports a package into an empty workspace and writes it again, to compare the bytes:
//!
//!     cargo run -p casual-note-package --example roundtrip -- in.cnotepack out.cnotepack
//!     cmp in.cnotepack out.cnotepack

use casual_note_merge::Workspace;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let (input, output) = (args.next().ok_or("in?")?, args.next().ok_or("out?")?);
    let bytes = std::fs::read(input)?;
    let created = casual_note_package::read_package(&bytes)?.created;
    let mut ws = Workspace::new(1)?;
    casual_note_package::import_package(&mut ws, &bytes, 2)?;
    std::fs::write(output, casual_note_package::export(&ws, &created, None)?)?;
    Ok(())
}
