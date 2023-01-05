use {
    crate::shred::{CodingShredHeader, DataShredHeader, Error, ShredCommonHeader},
    solana_sdk::{clock::Slot, signature::Signature},
};

pub(super) trait Shred: Sized {
    // Total size of payload including headers, merkle
    // branches (if any), zero paddings, etc.
    const SIZE_OF_PAYLOAD: usize;

    fn from_payload(shred: Vec<u8>) -> Result<Self, Error>;
    fn common_header(&self) -> &ShredCommonHeader;
    fn sanitize(&self) -> Result<(), Error>;

    fn set_signature(&mut self, signature: Signature);

    fn payload(&self) -> &Vec<u8>;
    fn into_payload(self) -> Vec<u8>;

    // Returns the shard index within the erasure coding set.
    fn erasure_shard_index(&self) -> Result<usize, Error>;
    // Returns the portion of the shred's payload which is erasure coded.
    fn erasure_shard(self) -> Result<Vec<u8>, Error>;
    // Like Shred::erasure_shard but returning a slice.
    fn erasure_shard_as_slice(&self) -> Result<&[u8], Error>;

    // Portion of the payload which is signed.
    fn signed_message(&self) -> &[u8];

    // Only for tests.
    fn set_index(&mut self, index: u32);
    fn set_slot(&mut self, slot: Slot);
}

pub(super) trait ShredData: Shred {
    fn data_header(&self) -> &DataShredHeader;

    fn parent(&self) -> Result<Slot, Error> {
        let slot = self.common_header().slot;
        let parent_offset = self.data_header().parent_offset;
        if parent_offset == 0 && slot != 0 {
            return Err(Error::InvalidParentOffset {
                slot,
                parent_offset,
            });
        }
        slot.checked_sub(Slot::from(parent_offset))
            .ok_or(Error::InvalidParentOffset {
                slot,
                parent_offset,
            })
    }

    fn data(&self) -> Result<&[u8], Error>;

    // Only for tests.
    fn set_last_in_slot(&mut self);
}

pub(super) trait ShredCode: Shred {
    fn coding_header(&self) -> &CodingShredHeader;

    fn first_coding_index(&self) -> Option<u32> {
        let position = u32::from(self.coding_header().position);
        self.common_header().index.checked_sub(position)
    }
}
