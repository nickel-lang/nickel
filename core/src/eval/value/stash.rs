use malachite::Natural;
use nickel_lang_parser::{
    ast::Number,
    files::{FileId, SerializeInterned},
};
use rkyv::{
    Archive, Deserialize, Serialize, SerializeUnsized,
    rancor::Fallible,
    rc::{ArchivedRc, Flavor, RcResolver},
    ser::sharing::SharingState,
};

use crate::{
    eval::value::{InlineValue, ValueBlockRc},
    position::PosIdx,
    term::Term,
};

use super::{
    ArrayData, CustomContractData, EnumVariantData, ForeignIdData, LabelData, NickelValue,
    RecordData, SealingKeyData, StringData, Thunk, TypeData, ValueContent,
};

#[derive(Archive, Serialize)]
#[rkyv(bytecheck(bounds(
    __C: rkyv::validation::ArchiveContext,
    __C: rkyv::validation::shared::SharedContext,
    <__C as rkyv::rancor::Fallible>::Error: rkyv::rancor::Source,
)))]
#[rkyv(serialize_bounds(
    __S: SerializeValue,
    __S::Error: rkyv::rancor::Source,
))]
pub struct ValueOwned {
    pos_idx: PosIdx,
    #[rkyv(omit_bounds)]
    payload: ValuePayload,
}

#[derive(Archive, Serialize)]
pub enum ValuePayload {
    Null,
    Bool(bool),
    Number(NumberStash),
    Array(ArrayData),
    Record(RecordData),
    String(StringData),
    Thunk(Thunk),
    Term(Term),
    Label(LabelData),
    EnumVariant(EnumVariantData),
    ForeignId(ForeignIdData),
    SealingKey(SealingKeyData),
    CustomContract(CustomContractData),
    Type(TypeData),
}

impl From<NickelValue> for ValueOwned {
    fn from(value: NickelValue) -> Self {
        let pos_idx = value.pos_idx();
        let payload = match value.content() {
            ValueContent::Null(_) => ValuePayload::Null,
            ValueContent::Bool(lens) => ValuePayload::Bool(lens.take()),
            ValueContent::Number(lens) => ValuePayload::Number(lens.take().into()),
            ValueContent::Array(lens) => ValuePayload::Array(lens.take().unwrap_or_alloc()),
            ValueContent::Record(lens) => ValuePayload::Record(lens.take().unwrap_or_alloc()),
            ValueContent::String(lens) => ValuePayload::String(lens.take()),
            ValueContent::Thunk(lens) => ValuePayload::Thunk(lens.take()),
            ValueContent::Term(term) => ValuePayload::Term(term.take()),
            ValueContent::Label(lens) => ValuePayload::Label(lens.take()),
            ValueContent::EnumVariant(lens) => ValuePayload::EnumVariant(lens.take()),
            ValueContent::ForeignId(lens) => ValuePayload::ForeignId(lens.take()),
            ValueContent::SealingKey(lens) => ValuePayload::SealingKey(lens.take()),
            ValueContent::CustomContract(lens) => ValuePayload::CustomContract(lens.take()),
            ValueContent::Type(lens) => ValuePayload::Type(lens.take()),
        };

        ValueOwned { pos_idx, payload }
    }
}

impl From<ValueBlockRc> for ValueOwned {
    fn from(v: ValueBlockRc) -> Self {
        ValueOwned::from(NickelValue::from(v))
    }
}

// TODO: with newer malachite (and some more code), we could do this without copying the number data.
#[derive(Archive, Serialize, Deserialize)]
pub struct NumberStash {
    sign: bool,
    num_limbs: Vec<u64>,
    denom_limbs: Vec<u64>,
}

impl From<Number> for NumberStash {
    fn from(n: Number) -> Self {
        let sign = n >= 0;
        let (num, denom) = n.into_numerator_and_denominator();
        NumberStash {
            sign,
            num_limbs: num.into_limbs_asc(),
            denom_limbs: denom.into_limbs_asc(),
        }
    }
}

impl From<NumberStash> for Number {
    fn from(nd: NumberStash) -> Self {
        Number::from_sign_and_naturals(
            nd.sign,
            Natural::from_owned_limbs_asc(nd.num_limbs),
            Natural::from_owned_limbs_asc(nd.denom_limbs),
        )
    }
}

pub struct NickelValueFlavor;

impl Flavor for NickelValueFlavor {
    // FIXME: what does this actually enable?
    const ALLOW_CYCLES: bool = true;
}

impl Archive for ValueBlockRc {
    type Archived = ArchivedRc<<ValueOwned as Archive>::Archived, NickelValueFlavor>;
    type Resolver = RcResolver;

    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        ArchivedRc::resolve_from_ref(&ValueOwned::from(self.clone()), resolver, out)
    }
}

impl Archive for NickelValue {
    type Archived = <NickelValueRepr as Archive>::Archived;
    type Resolver = <NickelValueRepr as Archive>::Resolver;

    fn resolve(&self, resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        NickelValueRepr::from(self.clone()).resolve(resolver, out)
    }
}

// For some reason, `derive(Archive)` on `InlineValue` doesn't like the repr(u32).
impl Archive for InlineValue {
    type Archived = rkyv::rend::u32_le;
    type Resolver = ();

    fn resolve(&self, _resolver: Self::Resolver, out: rkyv::Place<Self::Archived>) {
        out.write(rkyv::rend::u32_le::from_native(*self as u32));
    }
}

#[derive(Archive, Serialize)]
pub enum NickelValueRepr {
    Inline(InlineValue),
    Block(ValueBlockRc),
}

impl From<NickelValue> for NickelValueRepr {
    fn from(v: NickelValue) -> Self {
        if let Some(inline) = v.as_inline() {
            NickelValueRepr::Inline(inline)
        } else {
            // unwrap: conversion to inline failed, so it must be a block.
            NickelValueRepr::Block(v.try_into().unwrap())
        }
    }
}

pub trait SerializeValue:
    SerializeInterned<FileId>
    + SerializeInterned<PosIdx>
    + Fallible
    + rkyv::ser::Writer
    + rkyv::ser::Sharing
    + rkyv::ser::Allocator
{
}

impl<S> Serialize<S> for ValueBlockRc
where
    S: SerializeValue + ?Sized,
    S::Error: rkyv::rancor::Source,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, <S as Fallible>::Error> {
        // This is mostly copied from ArchivedRc::serialize_from_ref and
        // `SharingExt::serialize_shared`. The reason we need a slightly
        // different implementation is that we use the header address as the
        // Rc's identity, but then we don't just call serialize on the pointee
        // (because it's just the header).
        // FIXME: if we need cycle-breaking logic, I think this is where it goes
        let addr = self.header() as *const _ as usize;
        let pos = match serializer.start_sharing(addr) {
            SharingState::Started => {
                let pos = ValueOwned::from(self.clone()).serialize_unsized(serializer)?;
                serializer.finish_sharing(addr, pos)?;
                pos
            }
            SharingState::Pending => todo!(),
            SharingState::Finished(pos) => pos,
        };

        Ok(RcResolver::from_pos(pos))
    }
}

impl<S> Serialize<S> for InlineValue
where
    S: SerializeValue + ?Sized,
    S::Error: rkyv::rancor::Source,
{
    fn serialize(&self, _: &mut S) -> Result<Self::Resolver, <S as Fallible>::Error> {
        Ok(())
    }
}

impl<S> Serialize<S> for NickelValue
where
    S: SerializeValue + ?Sized,
    S::Error: rkyv::rancor::Source,
{
    fn serialize(&self, serializer: &mut S) -> Result<Self::Resolver, S::Error> {
        NickelValueRepr::from(self.clone()).serialize(serializer)
    }
}
