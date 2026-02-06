use std::ptr::NonNull;

use malachite::Natural;
use nickel_lang_parser::{
    ast::Number,
    files::{DeserializeInterned, FileId, SerializeInterned},
};
use rkyv::{
    Archive, Deserialize, Serialize, SerializeUnsized,
    de::{ErasedPtr, Pooling, PoolingExt},
    rancor::Fallible,
    rc::{ArchivedRc, Flavor, RcResolver},
    ser::sharing::SharingState,
};

use crate::{
    eval::value::{InlineValue, ValueBlockHeader, ValueBlockRc, lazy::ThunkData},
    position::PosIdx,
    term::Term,
};

use super::{
    ArrayData, CustomContractData, EnumVariantData, ForeignIdData, LabelData, NickelValue,
    RecordData, SealingKeyData, StringData, Thunk, TypeData, ValueContent,
};

#[derive(Archive, Serialize, Deserialize)]
#[rkyv(bytecheck(bounds(
    __C: rkyv::validation::ArchiveContext,
    __C: rkyv::validation::shared::SharedContext,
    <__C as rkyv::rancor::Fallible>::Error: rkyv::rancor::Source,
)))]
#[rkyv(serialize_bounds(
    __S: SerializeValue,
    __S::Error: rkyv::rancor::Source,
))]
#[rkyv(deserialize_bounds(
    __D: DeserializeValue,
    __D::Error: rkyv::rancor::Source
))]
pub struct ValueOwned {
    pos_idx: PosIdx,
    #[rkyv(omit_bounds)]
    payload: ValuePayload,
}

#[derive(Archive, Deserialize, Serialize)]
pub enum ValuePayload {
    Null,
    Bool(bool),
    Number(#[rkyv(with = nickel_lang_parser::stash::NumberStash)] Number),
    Array(ArrayData),
    Record(RecordData),
    String(StringData),
    // TODO: support thunks
    //Thunk(ThunkData),
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
            ValueContent::Number(lens) => ValuePayload::Number(lens.take()),
            ValueContent::Array(lens) => ValuePayload::Array(lens.take().unwrap_or_alloc()),
            ValueContent::Record(lens) => ValuePayload::Record(lens.take().unwrap_or_alloc()),
            ValueContent::String(lens) => ValuePayload::String(lens.take()),
            ValueContent::Thunk(_) => todo!(),
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

impl From<ValueOwned> for ValueBlockRc {
    fn from(v: ValueOwned) -> Self {
        match v.payload {
            ValuePayload::Null => todo!(),
            ValuePayload::Bool(_) => todo!(),
            ValuePayload::Number(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::Array(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::Record(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::String(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::Term(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::Label(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::EnumVariant(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::ForeignId(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::SealingKey(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::CustomContract(x) => ValueBlockRc::encode(x, v.pos_idx),
            ValuePayload::Type(x) => ValueBlockRc::encode(x, v.pos_idx),
        }
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

impl<D> Deserialize<InlineValue, D> for rkyv::rend::u32_le
where
    D: Fallible + ?Sized,
    D::Error: rkyv::rancor::Source,
{
    fn deserialize(&self, _deserializer: &mut D) -> Result<InlineValue, D::Error> {
        use rkyv::rancor::Source;
        self.to_native().try_into().map_err(D::Error::new)
    }
}

#[derive(Archive, Serialize)]
pub enum NickelValueRepr {
    Inline(InlineValue, PosIdx),
    Block(ValueBlockRc),
}

impl From<NickelValue> for NickelValueRepr {
    fn from(v: NickelValue) -> Self {
        if let Some(inline) = v.as_inline() {
            NickelValueRepr::Inline(inline, v.pos_idx())
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

pub trait DeserializeValue:
    DeserializeInterned<FileId> + DeserializeInterned<PosIdx> + Fallible + rkyv::de::Pooling
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

impl<D> Deserialize<ValueBlockRc, D> for ArchivedRc<ArchivedValueOwned, NickelValueFlavor>
where
    D: DeserializeValue + ?Sized,
    D::Error: rkyv::rancor::Source,
{
    fn deserialize(&self, deserializer: &mut D) -> Result<ValueBlockRc, <D as Fallible>::Error> {
        let address = self.get() as *const ArchivedValueOwned as usize;
        unsafe fn drop_rc_block(ptr: ErasedPtr) {
            let rc = unsafe {
                ValueBlockRc::from_raw(NonNull::new_unchecked(ptr.data_address() as *mut u8))
            };
            drop(rc)
        }
        match deserializer.start_pooling(address) {
            rkyv::de::PoolingState::Started => unsafe {
                // In principle, it should be possible to avoid a copy here by
                // using deserialize_unsized to deserialize straight into an
                // allocated value block.
                let value_owned = self.get().deserialize(deserializer)?;
                let rc = ValueBlockRc::from(value_owned);
                let ptr = ErasedPtr::new(NonNull::from(rc.header()));

                deserializer.finish_pooling(address, ptr, drop_rc_block)?;
                Ok(rc)
            },
            rkyv::de::PoolingState::Pending => todo!(), // FIXME: this means a reference cycle
            rkyv::de::PoolingState::Finished(ptr) => unsafe {
                let rc =
                    ValueBlockRc::from_raw(NonNull::new_unchecked(ptr.data_address() as *mut u8));
                rc.header().inc_ref_count();
                Ok(rc)
            },
        }
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

impl<D> Deserialize<NickelValue, D> for ArchivedNickelValueRepr
where
    D: DeserializeValue + ?Sized,
    D::Error: rkyv::rancor::Source,
{
    fn deserialize(&self, deserializer: &mut D) -> Result<NickelValue, <D as Fallible>::Error> {
        match self {
            ArchivedNickelValueRepr::Inline(i, pos) => Ok(NickelValue::inline(
                i.deserialize(deserializer)?,
                pos.deserialize(deserializer)?,
            )),
            ArchivedNickelValueRepr::Block(b) => {
                let value_block: ValueBlockRc = b.deserialize(deserializer)?;
                Ok(NickelValue::from(value_block))
            }
        }
    }
}
