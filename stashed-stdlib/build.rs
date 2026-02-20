use std::path::{Path, PathBuf};

use nickel_lang_core::{
    ast::{AstAlloc, compat::ToMainline},
    eval::value::NickelValue,
    files::Files,
    parser::{ErrorTolerantParser as _, TermParser, lexer::Lexer},
    position::PosTable,
    stash::Stasher,
    stdlib::StdlibModule,
};
use rkyv::{SerializeUnsized, ser::allocator::Arena};

pub fn main() {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("missing OUT_DIR variable"));
    std::fs::create_dir_all(&out_dir).expect("failed to create $OUT_DIR");

    write_one(
        StdlibModule::Internals,
        "<internals>",
        &out_dir.join("internals.bin"),
    );
    write_one(StdlibModule::Std, "<stdlib>", &out_dir.join("stdlib.bin"));
}

fn write_one(module: StdlibModule, name: &str, path: &Path) {
    let mut files = Files::empty();
    let id = files.add(name, module.content());
    let alloc = AstAlloc::new();
    let mut pos_table = PosTable::default();
    let mut arena = Arena::new();
    let ast = TermParser::new()
        .parse_strict(&alloc, id, Lexer::new(module.content()))
        .unwrap();

    let value: NickelValue = ast.to_mainline(&mut pos_table);

    let mut stasher = Stasher::new(&pos_table, [id], arena.acquire());
    value.serialize_unsized(&mut stasher).unwrap();

    std::fs::write(path, stasher.into_bytes()).unwrap();
}
