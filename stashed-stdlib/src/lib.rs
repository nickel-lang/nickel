use nickel_lang_core::{
    eval::value::NickelValue, files::Files, position::PosTable, stash::Unstasher,
};
use rkyv::{Archive, access, api::deserialize_using, util::AlignedVec};

const INTERNALS: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/internals.bin"));
const STDLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stdlib.bin"));

pub struct Stdlib {
    pub files: Files,
    pub pos_table: PosTable,
    //std: NickelValue,
    pub internals: NickelValue,
}

pub fn unstash_stdlib() -> Stdlib {
    let files = Files::new(
        nickel_lang_core::stdlib::modules().map(|m| (m.file_name().to_owned(), m.content())),
    );

    let mut unstasher = Unstasher::new(files.stdlib_modules(), PosTable::new());
    let mut internals_aligned = AlignedVec::<256>::with_capacity(INTERNALS.len());
    internals_aligned.extend_from_slice(INTERNALS);
    let internals_archived =
        access::<<NickelValue as Archive>::Archived, rkyv::rancor::Error>(&internals_aligned)
            .unwrap();
    let internals =
        deserialize_using::<_, _, rkyv::rancor::Error>(internals_archived, &mut unstasher).unwrap();
    Stdlib {
        files,
        pos_table: unstasher.pos_table,
        internals,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unstashes_without_crashing() {
        super::unstash_stdlib();
    }
}
