use nickel_lang_core::{files::Files, position::PosTable};

const INTERNALS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/internals.bin"));
const STDLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stdlib.bin"));

pub fn unstash_stdlib(pos_table: &mut PosTable) -> Files {
    todo!()
}
