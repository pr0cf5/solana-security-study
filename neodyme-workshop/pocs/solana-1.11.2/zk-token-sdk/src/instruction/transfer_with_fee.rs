use {
    crate::zk_token_elgamal::pod,
    bytemuck::{Pod, Zeroable},
};
#[cfg(not(target_os = "solana"))]
use {
    crate::{
        encryption::{
            elgamal::{
                DecryptHandle, ElGamalCiphertext, ElGamalKeypair, ElGamalPubkey, ElGamalSecretKey,
            },
            pedersen::{Pedersen, PedersenCommitment, PedersenOpening},
        },
        errors::ProofError,
        instruction::{
            combine_lo_hi_ciphertexts, combine_lo_hi_commitments, combine_lo_hi_openings,
            split_u64, transfer::TransferAmountEncryption, Role, Verifiable,
        },
        range_proof::RangeProof,
        sigma_proofs::{
            equality_proof::CtxtCommEqualityProof,
            fee_proof::FeeSigmaProof,
            validity_proof::{AggregatedValidityProof, ValidityProof},
        },
        transcript::TranscriptProtocol,
    },
    arrayref::{array_ref, array_refs},
    curve25519_dalek::scalar::Scalar,
    merlin::Transcript,
    std::convert::TryInto,
    subtle::{ConditionallySelectable, ConstantTimeGreater},
};

#[cfg(not(target_os = "solana"))]
const MAX_FEE_BASIS_POINTS: u64 = 10_000;
#[cfg(not(target_os = "solana"))]
const ONE_IN_BASIS_POINTS: u128 = MAX_FEE_BASIS_POINTS as u128;

#[cfg(not(target_os = "solana"))]
const TRANSFER_SOURCE_AMOUNT_BITS: usize = 64;
#[cfg(not(target_os = "solana"))]
const TRANSFER_AMOUNT_LO_BITS: usize = 16;
#[cfg(not(target_os = "solana"))]
const TRANSFER_AMOUNT_LO_NEGATED_BITS: usize = 16;
#[cfg(not(target_os = "solana"))]
const TRANSFER_AMOUNT_HI_BITS: usize = 32;
#[cfg(not(target_os = "solana"))]
const TRANSFER_DELTA_BITS: usize = 64;

#[cfg(not(target_os = "solana"))]
lazy_static::lazy_static! {
    pub static ref COMMITMENT_MAX: PedersenCommitment = Pedersen::encode((1_u64 <<
                                                                         TRANSFER_AMOUNT_LO_NEGATED_BITS) - 1);
    pub static ref COMMITMENT_MAX_FEE_BASIS_POINTS: PedersenCommitment = Pedersen::encode(MAX_FEE_BASIS_POINTS);
}

// #[derive(Clone, Copy, Pod, Zeroable)]
#[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
pub struct TransferWithFeeData {
    /// Group encryption of the low 32 bites of the transfer amount
    pub ciphertext_lo: pod::TransferAmountEncryption,

    /// Group encryption of the high 32 bits of the transfer amount
    pub ciphertext_hi: pod::TransferAmountEncryption,

    /// The public encryption keys associated with the transfer: source, dest, and auditor
    pub transfer_with_fee_pubkeys: pod::TransferWithFeePubkeys,

    /// The final spendable ciphertext after the transfer,
    pub new_source_ciphertext: pod::ElGamalCiphertext,

    // transfer fee encryption
    pub fee_ciphertext: pod::FeeEncryption,

    // fee parameters
    pub fee_parameters: pod::FeeParameters,

    // transfer fee proof
    pub proof: TransferWithFeeProof,
}

#[cfg(not(target_os = "solana"))]
impl TransferWithFeeData {
    pub fn new(
        transfer_amount: u64,
        (spendable_balance, old_source_ciphertext): (u64, &ElGamalCiphertext),
        source_keypair: &ElGamalKeypair,
        (destination_pubkey, auditor_pubkey): (&ElGamalPubkey, &ElGamalPubkey),
        fee_parameters: FeeParameters,
        withdraw_withheld_authority_pubkey: &ElGamalPubkey,
    ) -> Result<Self, ProofError> {
        // split and encrypt transfer amount
        let (amount_lo, amount_hi) = split_u64(
            transfer_amount,
            TRANSFER_AMOUNT_LO_BITS,
            TRANSFER_AMOUNT_HI_BITS,
        )?;

        let (ciphertext_lo, opening_lo) = TransferAmountEncryption::new(
            amount_lo,
            &source_keypair.public,
            destination_pubkey,
            auditor_pubkey,
        );
        let (ciphertext_hi, opening_hi) = TransferAmountEncryption::new(
            amount_hi,
            &source_keypair.public,
            destination_pubkey,
            auditor_pubkey,
        );

        // subtract transfer amount from the spendable ciphertext
        let new_spendable_balance = spendable_balance
            .checked_sub(transfer_amount)
            .ok_or(ProofError::Generation)?;

        let transfer_amount_lo_source = ElGamalCiphertext {
            commitment: ciphertext_lo.commitment,
            handle: ciphertext_lo.source_handle,
        };

        let transfer_amount_hi_source = ElGamalCiphertext {
            commitment: ciphertext_hi.commitment,
            handle: ciphertext_hi.source_handle,
        };

        let new_source_ciphertext = old_source_ciphertext
            - combine_lo_hi_ciphertexts(
                &transfer_amount_lo_source,
                &transfer_amount_hi_source,
                TRANSFER_AMOUNT_LO_BITS,
            );

        // calculate and encrypt fee
        let (fee_amount, delta_fee) =
            calculate_fee(transfer_amount, fee_parameters.fee_rate_basis_points)
                .ok_or(ProofError::Generation)?;

        let below_max = u64::ct_gt(&fee_parameters.maximum_fee, &fee_amount);
        let fee_to_encrypt =
            u64::conditional_select(&fee_parameters.maximum_fee, &fee_amount, below_max);

        let (fee_ciphertext, opening_fee) = FeeEncryption::new(
            fee_to_encrypt,
            destination_pubkey,
            withdraw_withheld_authority_pubkey,
        );

        // generate transcript and append all public inputs
        let pod_transfer_with_fee_pubkeys = pod::TransferWithFeePubkeys {
            source_pubkey: source_keypair.public.into(),
            destination_pubkey: (*destination_pubkey).into(),
            auditor_pubkey: (*auditor_pubkey).into(),
            withdraw_withheld_authority_pubkey: (*withdraw_withheld_authority_pubkey).into(),
        };
        let pod_ciphertext_lo: pod::TransferAmountEncryption = ciphertext_lo.to_pod();
        let pod_ciphertext_hi: pod::TransferAmountEncryption = ciphertext_hi.to_pod();
        let pod_new_source_ciphertext: pod::ElGamalCiphertext = new_source_ciphertext.into();
        let pod_fee_ciphertext: pod::FeeEncryption = fee_ciphertext.to_pod();

        let mut transcript = TransferWithFeeProof::transcript_new(
            &pod_transfer_with_fee_pubkeys,
            &pod_ciphertext_lo,
            &pod_ciphertext_hi,
            &pod_new_source_ciphertext,
            &pod_fee_ciphertext,
        );

        let proof = TransferWithFeeProof::new(
            (amount_lo, &ciphertext_lo, &opening_lo),
            (amount_hi, &ciphertext_hi, &opening_hi),
            source_keypair,
            (destination_pubkey, auditor_pubkey),
            (new_spendable_balance, &new_source_ciphertext),
            (fee_to_encrypt, &fee_ciphertext, &opening_fee),
            delta_fee,
            withdraw_withheld_authority_pubkey,
            fee_parameters,
            &mut transcript,
        );

        Ok(Self {
            ciphertext_lo: pod_ciphertext_lo,
            ciphertext_hi: pod_ciphertext_hi,
            transfer_with_fee_pubkeys: pod_transfer_with_fee_pubkeys,
            new_source_ciphertext: pod_new_source_ciphertext,
            fee_ciphertext: pod_fee_ciphertext,
            fee_parameters: fee_parameters.into(),
            proof,
        })
    }

    /// Extracts the lo ciphertexts associated with a transfer-with-fee data
    fn ciphertext_lo(&self, role: Role) -> Result<ElGamalCiphertext, ProofError> {
        let ciphertext_lo: TransferAmountEncryption = self.ciphertext_lo.try_into()?;

        let handle_lo = match role {
            Role::Source => ciphertext_lo.source_handle,
            Role::Dest => ciphertext_lo.destination_handle,
            Role::Auditor => ciphertext_lo.auditor_handle,
        };

        Ok(ElGamalCiphertext {
            commitment: ciphertext_lo.commitment,
            handle: handle_lo,
        })
    }

    /// Extracts the lo ciphertexts associated with a transfer-with-fee data
    fn ciphertext_hi(&self, role: Role) -> Result<ElGamalCiphertext, ProofError> {
        let ciphertext_hi: TransferAmountEncryption = self.ciphertext_hi.try_into()?;

        let handle_hi = match role {
            Role::Source => ciphertext_hi.source_handle,
            Role::Dest => ciphertext_hi.destination_handle,
            Role::Auditor => ciphertext_hi.auditor_handle,
        };

        Ok(ElGamalCiphertext {
            commitment: ciphertext_hi.commitment,
            handle: handle_hi,
        })
    }

    /// Decrypts transfer amount from transfer-with-fee data
    pub fn decrypt_amount(&self, role: Role, sk: &ElGamalSecretKey) -> Result<u64, ProofError> {
        let ciphertext_lo = self.ciphertext_lo(role)?;
        let ciphertext_hi = self.ciphertext_hi(role)?;

        let amount_lo = ciphertext_lo.decrypt_u32(sk);
        let amount_hi = ciphertext_hi.decrypt_u32(sk);

        if let (Some(amount_lo), Some(amount_hi)) = (amount_lo, amount_hi) {
            let two_power = 1 << TRANSFER_AMOUNT_LO_BITS;
            Ok(amount_lo + two_power * amount_hi)
        } else {
            Err(ProofError::Verification)
        }
    }
}

#[cfg(not(target_os = "solana"))]
impl Verifiable for TransferWithFeeData {
    fn verify(&self) -> Result<(), ProofError> {
        let mut transcript = TransferWithFeeProof::transcript_new(
            &self.transfer_with_fee_pubkeys,
            &self.ciphertext_lo,
            &self.ciphertext_hi,
            &self.new_source_ciphertext,
            &self.fee_ciphertext,
        );

        let ciphertext_lo = self.ciphertext_lo.try_into()?;
        let ciphertext_hi = self.ciphertext_hi.try_into()?;
        let pubkeys_transfer_with_fee = self.transfer_with_fee_pubkeys.try_into()?;
        let new_source_ciphertext = self.new_source_ciphertext.try_into()?;

        let fee_ciphertext = self.fee_ciphertext.try_into()?;
        let fee_parameters = self.fee_parameters.into();

        self.proof.verify(
            &ciphertext_lo,
            &ciphertext_hi,
            &pubkeys_transfer_with_fee,
            &new_source_ciphertext,
            &fee_ciphertext,
            fee_parameters,
            &mut transcript,
        )
    }
}

// #[derive(Clone, Copy, Pod, Zeroable)]
#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct TransferWithFeeProof {
    pub new_source_commitment: pod::PedersenCommitment,
    pub claimed_commitment: pod::PedersenCommitment,
    pub equality_proof: pod::CtxtCommEqualityProof,
    pub ciphertext_amount_validity_proof: pod::AggregatedValidityProof,
    pub fee_sigma_proof: pod::FeeSigmaProof,
    pub fee_ciphertext_validity_proof: pod::ValidityProof,
    pub range_proof: pod::RangeProof256,
}

#[allow(non_snake_case)]
#[cfg(not(target_os = "solana"))]
impl TransferWithFeeProof {
    fn transcript_new(
        transfer_with_fee_pubkeys: &pod::TransferWithFeePubkeys,
        ciphertext_lo: &pod::TransferAmountEncryption,
        ciphertext_hi: &pod::TransferAmountEncryption,
        new_source_ciphertext: &pod::ElGamalCiphertext,
        fee_ciphertext: &pod::FeeEncryption,
    ) -> Transcript {
        let mut transcript = Transcript::new(b"FeeProof");

        transcript.append_pubkey(b"pubkey-source", &transfer_with_fee_pubkeys.source_pubkey);
        transcript.append_pubkey(
            b"pubkey-dest",
            &transfer_with_fee_pubkeys.destination_pubkey,
        );
        transcript.append_pubkey(b"pubkey-auditor", &transfer_with_fee_pubkeys.auditor_pubkey);
        transcript.append_pubkey(
            b"withdraw_withheld_authority_pubkey",
            &transfer_with_fee_pubkeys.withdraw_withheld_authority_pubkey,
        );

        transcript.append_commitment(b"comm-lo-amount", &ciphertext_lo.commitment);
        transcript.append_handle(b"handle-lo-source", &ciphertext_lo.source_handle);
        transcript.append_handle(b"handle-lo-dest", &ciphertext_lo.destination_handle);
        transcript.append_handle(b"handle-lo-auditor", &ciphertext_lo.auditor_handle);

        transcript.append_commitment(b"comm-hi-amount", &ciphertext_hi.commitment);
        transcript.append_handle(b"handle-hi-source", &ciphertext_hi.source_handle);
        transcript.append_handle(b"handle-hi-dest", &ciphertext_hi.destination_handle);
        transcript.append_handle(b"handle-hi-auditor", &ciphertext_hi.auditor_handle);

        transcript.append_ciphertext(b"ctxt-new-source", new_source_ciphertext);

        transcript.append_commitment(b"comm-fee", &fee_ciphertext.commitment);
        transcript.append_handle(b"fee-dest-handle", &fee_ciphertext.destination_handle);
        transcript.append_handle(
            b"handle-fee-auditor",
            &fee_ciphertext.withdraw_withheld_authority_handle,
        );

        transcript
    }

    #[allow(clippy::too_many_arguments)]
    #[allow(clippy::many_single_char_names)]
    pub fn new(
        transfer_amount_lo_data: (u64, &TransferAmountEncryption, &PedersenOpening),
        transfer_amount_hi_data: (u64, &TransferAmountEncryption, &PedersenOpening),
        source_keypair: &ElGamalKeypair,
        (destination_pubkey, auditor_pubkey): (&ElGamalPubkey, &ElGamalPubkey),
        (source_new_balance, new_source_ciphertext): (u64, &ElGamalCiphertext),

        (fee_amount, fee_ciphertext, opening_fee): (u64, &FeeEncryption, &PedersenOpening),
        delta_fee: u64,
        withdraw_withheld_authority_pubkey: &ElGamalPubkey,
        fee_parameters: FeeParameters,
        transcript: &mut Transcript,
    ) -> Self {
        let (transfer_amount_lo, ciphertext_lo, opening_lo) = transfer_amount_lo_data;
        let (transfer_amount_hi, ciphertext_hi, opening_hi) = transfer_amount_hi_data;

        // generate a Pedersen commitment for the remaining balance in source
        let (new_source_commitment, opening_source) = Pedersen::new(source_new_balance);
        let (claimed_commitment, opening_claimed) = Pedersen::new(delta_fee);

        let pod_new_source_commitment: pod::PedersenCommitment = new_source_commitment.into();
        let pod_claimed_commitment: pod::PedersenCommitment = claimed_commitment.into();

        transcript.append_commitment(b"commitment-new-source", &pod_new_source_commitment);
        transcript.append_commitment(b"commitment-claimed", &pod_claimed_commitment);

        // generate equality_proof
        let equality_proof = CtxtCommEqualityProof::new(
            source_keypair,
            new_source_ciphertext,
            source_new_balance,
            &opening_source,
            transcript,
        );

        // generate ciphertext validity proof
        let ciphertext_amount_validity_proof = AggregatedValidityProof::new(
            (destination_pubkey, auditor_pubkey),
            (transfer_amount_lo, transfer_amount_hi),
            (opening_lo, opening_hi),
            transcript,
        );

        let (delta_commitment, opening_delta) = compute_delta_commitment_and_opening(
            (&ciphertext_lo.commitment, opening_lo),
            (&ciphertext_hi.commitment, opening_hi),
            (&fee_ciphertext.commitment, opening_fee),
            fee_parameters.fee_rate_basis_points,
        );

        let fee_sigma_proof = FeeSigmaProof::new(
            (fee_amount, &fee_ciphertext.commitment, opening_fee),
            (delta_fee, &delta_commitment, &opening_delta),
            (&claimed_commitment, &opening_claimed),
            fee_parameters.maximum_fee,
            transcript,
        );

        let fee_ciphertext_validity_proof = ValidityProof::new(
            (destination_pubkey, withdraw_withheld_authority_pubkey),
            fee_amount,
            opening_fee,
            transcript,
        );

        // generate the range proof
        let opening_claimed_negated = &PedersenOpening::default() - &opening_claimed;
        let range_proof = if TRANSFER_AMOUNT_LO_BITS == 32 {
            RangeProof::new(
                vec![
                    source_new_balance,
                    transfer_amount_lo as u64,
                    transfer_amount_hi as u64,
                    delta_fee,
                    MAX_FEE_BASIS_POINTS - delta_fee,
                ],
                vec![
                    TRANSFER_SOURCE_AMOUNT_BITS,
                    TRANSFER_AMOUNT_LO_BITS,
                    TRANSFER_AMOUNT_HI_BITS,
                    TRANSFER_DELTA_BITS,
                    TRANSFER_DELTA_BITS,
                ],
                vec![
                    &opening_source,
                    opening_lo,
                    opening_hi,
                    &opening_claimed,
                    &opening_claimed_negated,
                ],
                transcript,
            )
        } else {
            let transfer_amount_lo_negated =
                ((1 << TRANSFER_AMOUNT_LO_NEGATED_BITS) - 1) - transfer_amount_lo as u64;
            let opening_lo_negated = &PedersenOpening::default() - opening_lo;

            RangeProof::new(
                vec![
                    source_new_balance,
                    transfer_amount_lo as u64,
                    transfer_amount_lo_negated,
                    transfer_amount_hi as u64,
                    delta_fee,
                    MAX_FEE_BASIS_POINTS - delta_fee,
                ],
                vec![
                    TRANSFER_SOURCE_AMOUNT_BITS,
                    TRANSFER_AMOUNT_LO_BITS,
                    TRANSFER_AMOUNT_LO_NEGATED_BITS,
                    TRANSFER_AMOUNT_HI_BITS,
                    TRANSFER_DELTA_BITS,
                    TRANSFER_DELTA_BITS,
                ],
                vec![
                    &opening_source,
                    opening_lo,
                    &opening_lo_negated,
                    opening_hi,
                    &opening_claimed,
                    &opening_claimed_negated,
                ],
                transcript,
            )
        };

        Self {
            new_source_commitment: pod_new_source_commitment,
            claimed_commitment: pod_claimed_commitment,
            equality_proof: equality_proof.into(),
            ciphertext_amount_validity_proof: ciphertext_amount_validity_proof.into(),
            fee_sigma_proof: fee_sigma_proof.into(),
            fee_ciphertext_validity_proof: fee_ciphertext_validity_proof.into(),
            range_proof: range_proof.try_into().expect("range proof: length error"),
        }
    }

    pub fn verify(
        &self,
        ciphertext_lo: &TransferAmountEncryption,
        ciphertext_hi: &TransferAmountEncryption,
        transfer_with_fee_pubkeys: &TransferWithFeePubkeys,
        new_spendable_ciphertext: &ElGamalCiphertext,

        fee_ciphertext: &FeeEncryption,
        fee_parameters: FeeParameters,
        transcript: &mut Transcript,
    ) -> Result<(), ProofError> {
        transcript.append_commitment(b"commitment-new-source", &self.new_source_commitment);
        transcript.append_commitment(b"commitment-claimed", &self.claimed_commitment);

        let new_source_commitment: PedersenCommitment = self.new_source_commitment.try_into()?;
        let claimed_commitment: PedersenCommitment = self.claimed_commitment.try_into()?;

        let equality_proof: CtxtCommEqualityProof = self.equality_proof.try_into()?;
        let ciphertext_amount_validity_proof: AggregatedValidityProof =
            self.ciphertext_amount_validity_proof.try_into()?;
        let fee_sigma_proof: FeeSigmaProof = self.fee_sigma_proof.try_into()?;
        let fee_ciphertext_validity_proof: ValidityProof =
            self.fee_ciphertext_validity_proof.try_into()?;
        let range_proof: RangeProof = self.range_proof.try_into()?;

        // verify equality proof
        equality_proof.verify(
            &transfer_with_fee_pubkeys.source_pubkey,
            new_spendable_ciphertext,
            &new_source_commitment,
            transcript,
        )?;

        // verify that the transfer amount is encrypted correctly
        ciphertext_amount_validity_proof.verify(
            (
                &transfer_with_fee_pubkeys.destination_pubkey,
                &transfer_with_fee_pubkeys.auditor_pubkey,
            ),
            (&ciphertext_lo.commitment, &ciphertext_hi.commitment),
            (
                &ciphertext_lo.destination_handle,
                &ciphertext_hi.destination_handle,
            ),
            (&ciphertext_lo.auditor_handle, &ciphertext_hi.auditor_handle),
            transcript,
        )?;

        // verify fee sigma proof
        let delta_commitment = compute_delta_commitment(
            &ciphertext_lo.commitment,
            &ciphertext_hi.commitment,
            &fee_ciphertext.commitment,
            fee_parameters.fee_rate_basis_points,
        );

        fee_sigma_proof.verify(
            &fee_ciphertext.commitment,
            &delta_commitment,
            &claimed_commitment,
            fee_parameters.maximum_fee,
            transcript,
        )?;

        fee_ciphertext_validity_proof.verify(
            &fee_ciphertext.commitment,
            (
                &transfer_with_fee_pubkeys.destination_pubkey,
                &transfer_with_fee_pubkeys.withdraw_withheld_authority_pubkey,
            ),
            (
                &fee_ciphertext.destination_handle,
                &fee_ciphertext.withdraw_withheld_authority_handle,
            ),
            transcript,
        )?;

        // verify range proof
        let new_source_commitment = self.new_source_commitment.try_into()?;
        let claimed_commitment_negated = &(*COMMITMENT_MAX_FEE_BASIS_POINTS) - &claimed_commitment;

        if TRANSFER_AMOUNT_LO_BITS == 32 {
            range_proof.verify(
                vec![
                    &new_source_commitment,
                    &ciphertext_lo.commitment,
                    &ciphertext_hi.commitment,
                    &claimed_commitment,
                    &claimed_commitment_negated,
                ],
                vec![64, 32, 32, 64, 64],
                transcript,
            )?;
        } else {
            let commitment_lo_negated = &(*COMMITMENT_MAX) - &ciphertext_lo.commitment;

            range_proof.verify(
                vec![
                    &new_source_commitment,
                    &ciphertext_lo.commitment,
                    &commitment_lo_negated,
                    &ciphertext_hi.commitment,
                    &claimed_commitment,
                    &claimed_commitment_negated,
                ],
                vec![64, 16, 16, 32, 64, 64],
                transcript,
            )?;
        }

        Ok(())
    }
}

/// The ElGamal public keys needed for a transfer with fee
#[derive(Clone)]
#[repr(C)]
#[cfg(not(target_os = "solana"))]
pub struct TransferWithFeePubkeys {
    pub source_pubkey: ElGamalPubkey,
    pub destination_pubkey: ElGamalPubkey,
    pub auditor_pubkey: ElGamalPubkey,
    pub withdraw_withheld_authority_pubkey: ElGamalPubkey,
}

#[cfg(not(target_os = "solana"))]
impl TransferWithFeePubkeys {
    pub fn to_bytes(&self) -> [u8; 128] {
        let mut bytes = [0u8; 128];
        bytes[..32].copy_from_slice(&self.source_pubkey.to_bytes());
        bytes[32..64].copy_from_slice(&self.destination_pubkey.to_bytes());
        bytes[64..96].copy_from_slice(&self.auditor_pubkey.to_bytes());
        bytes[96..128].copy_from_slice(&self.withdraw_withheld_authority_pubkey.to_bytes());
        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProofError> {
        let bytes = array_ref![bytes, 0, 128];
        let (source_pubkey, destination_pubkey, auditor_pubkey, withdraw_withheld_authority_pubkey) =
            array_refs![bytes, 32, 32, 32, 32];

        let source_pubkey =
            ElGamalPubkey::from_bytes(source_pubkey).ok_or(ProofError::Verification)?;
        let destination_pubkey =
            ElGamalPubkey::from_bytes(destination_pubkey).ok_or(ProofError::Verification)?;
        let auditor_pubkey =
            ElGamalPubkey::from_bytes(auditor_pubkey).ok_or(ProofError::Verification)?;
        let withdraw_withheld_authority_pubkey =
            ElGamalPubkey::from_bytes(withdraw_withheld_authority_pubkey)
                .ok_or(ProofError::Verification)?;

        Ok(Self {
            source_pubkey,
            destination_pubkey,
            auditor_pubkey,
            withdraw_withheld_authority_pubkey,
        })
    }
}

#[derive(Clone)]
#[repr(C)]
#[cfg(not(target_os = "solana"))]
pub struct FeeEncryption {
    pub commitment: PedersenCommitment,
    pub destination_handle: DecryptHandle,
    pub withdraw_withheld_authority_handle: DecryptHandle,
}

#[cfg(not(target_os = "solana"))]
impl FeeEncryption {
    pub fn new(
        amount: u64,
        destination_pubkey: &ElGamalPubkey,
        withdraw_withheld_authority_pubkey: &ElGamalPubkey,
    ) -> (Self, PedersenOpening) {
        let (commitment, opening) = Pedersen::new(amount);
        let fee_encryption = Self {
            commitment,
            destination_handle: destination_pubkey.decrypt_handle(&opening),
            withdraw_withheld_authority_handle: withdraw_withheld_authority_pubkey
                .decrypt_handle(&opening),
        };

        (fee_encryption, opening)
    }

    pub fn to_pod(&self) -> pod::FeeEncryption {
        pod::FeeEncryption {
            commitment: self.commitment.into(),
            destination_handle: self.destination_handle.into(),
            withdraw_withheld_authority_handle: self.withdraw_withheld_authority_handle.into(),
        }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct FeeParameters {
    /// Fee rate expressed as basis points of the transfer amount, i.e. increments of 0.01%
    pub fee_rate_basis_points: u16,
    /// Maximum fee assessed on transfers, expressed as an amount of tokens
    pub maximum_fee: u64,
}

#[cfg(not(target_os = "solana"))]
impl FeeParameters {
    pub fn to_bytes(&self) -> [u8; 10] {
        let mut bytes = [0u8; 10];
        bytes[..2].copy_from_slice(&self.fee_rate_basis_points.to_le_bytes());
        bytes[2..10].copy_from_slice(&self.maximum_fee.to_le_bytes());

        bytes
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        let bytes = array_ref![bytes, 0, 10];
        let (fee_rate_basis_points, maximum_fee) = array_refs![bytes, 2, 8];

        Self {
            fee_rate_basis_points: u16::from_le_bytes(*fee_rate_basis_points),
            maximum_fee: u64::from_le_bytes(*maximum_fee),
        }
    }
}

#[cfg(not(target_os = "solana"))]
fn calculate_fee(transfer_amount: u64, fee_rate_basis_points: u16) -> Option<(u64, u64)> {
    let numerator = (transfer_amount as u128).checked_mul(fee_rate_basis_points as u128)?;
    let mut fee = numerator.checked_div(ONE_IN_BASIS_POINTS)?;
    let mut delta_fee = 0_u128;

    let remainder = numerator.checked_rem(ONE_IN_BASIS_POINTS)?;
    if remainder > 0 {
        fee = fee.checked_add(1)?;

        let scaled_fee = fee.checked_mul(ONE_IN_BASIS_POINTS)?;
        delta_fee = scaled_fee.checked_sub(numerator)?;
    }

    let fee = u64::try_from(fee).ok()?;
    Some((fee as u64, delta_fee as u64))
}

#[cfg(not(target_os = "solana"))]
fn compute_delta_commitment_and_opening(
    (commitment_lo, opening_lo): (&PedersenCommitment, &PedersenOpening),
    (commitment_hi, opening_hi): (&PedersenCommitment, &PedersenOpening),
    (fee_commitment, opening_fee): (&PedersenCommitment, &PedersenOpening),
    fee_rate_basis_points: u16,
) -> (PedersenCommitment, PedersenOpening) {
    let fee_rate_scalar = Scalar::from(fee_rate_basis_points);

    let delta_commitment = fee_commitment * Scalar::from(MAX_FEE_BASIS_POINTS)
        - &(&combine_lo_hi_commitments(commitment_lo, commitment_hi, TRANSFER_AMOUNT_LO_BITS)
            * &fee_rate_scalar);

    let opening_delta = opening_fee * Scalar::from(MAX_FEE_BASIS_POINTS)
        - &(&combine_lo_hi_openings(opening_lo, opening_hi, TRANSFER_AMOUNT_LO_BITS)
            * &fee_rate_scalar);

    (delta_commitment, opening_delta)
}

#[cfg(not(target_os = "solana"))]
fn compute_delta_commitment(
    commitment_lo: &PedersenCommitment,
    commitment_hi: &PedersenCommitment,
    fee_commitment: &PedersenCommitment,
    fee_rate_basis_points: u16,
) -> PedersenCommitment {
    let fee_rate_scalar = Scalar::from(fee_rate_basis_points);

    fee_commitment * Scalar::from(MAX_FEE_BASIS_POINTS)
        - &(&combine_lo_hi_commitments(commitment_lo, commitment_hi, TRANSFER_AMOUNT_LO_BITS)
            * &fee_rate_scalar)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_fee_correctness() {
        let source_keypair = ElGamalKeypair::new_rand();
        let destination_pubkey = ElGamalKeypair::new_rand().public;
        let auditor_pubkey = ElGamalKeypair::new_rand().public;
        let withdraw_withheld_authority_pubkey = ElGamalKeypair::new_rand().public;

        // Case 1: transfer 0 amount
        let spendable_balance: u64 = 120;
        let spendable_ciphertext = source_keypair.public.encrypt(spendable_balance);

        let transfer_amount: u64 = 0;

        let fee_parameters = FeeParameters {
            fee_rate_basis_points: 400,
            maximum_fee: 3,
        };

        let fee_data = TransferWithFeeData::new(
            transfer_amount,
            (spendable_balance, &spendable_ciphertext),
            &source_keypair,
            (&destination_pubkey, &auditor_pubkey),
            fee_parameters,
            &withdraw_withheld_authority_pubkey,
        )
        .unwrap();

        assert!(fee_data.verify().is_ok());

        // Case 2: transfer max amount
        let spendable_balance: u64 = u64::max_value();
        let spendable_ciphertext = source_keypair.public.encrypt(spendable_balance);

        let transfer_amount: u64 =
            (1u64 << (TRANSFER_AMOUNT_LO_BITS + TRANSFER_AMOUNT_HI_BITS)) - 1;

        let fee_parameters = FeeParameters {
            fee_rate_basis_points: 400,
            maximum_fee: 3,
        };

        let fee_data = TransferWithFeeData::new(
            transfer_amount,
            (spendable_balance, &spendable_ciphertext),
            &source_keypair,
            (&destination_pubkey, &auditor_pubkey),
            fee_parameters,
            &withdraw_withheld_authority_pubkey,
        )
        .unwrap();

        assert!(fee_data.verify().is_ok());

        // Case 3: general success case
        let spendable_balance: u64 = 120;
        let spendable_ciphertext = source_keypair.public.encrypt(spendable_balance);

        let transfer_amount: u64 = 100;

        let fee_parameters = FeeParameters {
            fee_rate_basis_points: 400,
            maximum_fee: 3,
        };

        let fee_data = TransferWithFeeData::new(
            transfer_amount,
            (spendable_balance, &spendable_ciphertext),
            &source_keypair,
            (&destination_pubkey, &auditor_pubkey),
            fee_parameters,
            &withdraw_withheld_authority_pubkey,
        )
        .unwrap();

        assert!(fee_data.verify().is_ok());

        // Case 4: transfer amount too big
        let spendable_balance: u64 = u64::max_value();
        let spendable_ciphertext = source_keypair.public.encrypt(spendable_balance);

        let transfer_amount: u64 = 1u64 << (TRANSFER_AMOUNT_LO_BITS + TRANSFER_AMOUNT_HI_BITS);

        let fee_parameters = FeeParameters {
            fee_rate_basis_points: 400,
            maximum_fee: 3,
        };

        let fee_data = TransferWithFeeData::new(
            transfer_amount,
            (spendable_balance, &spendable_ciphertext),
            &source_keypair,
            (&destination_pubkey, &auditor_pubkey),
            fee_parameters,
            &withdraw_withheld_authority_pubkey,
        );

        assert!(fee_data.is_err());

        // Case 5: invalid destination, auditor, or withdraw authority pubkeys
        let spendable_balance: u64 = 120;
        let spendable_ciphertext = source_keypair.public.encrypt(spendable_balance);

        let transfer_amount: u64 = 0;

        let fee_parameters = FeeParameters {
            fee_rate_basis_points: 400,
            maximum_fee: 3,
        };

        // destination pubkey invalid
        let destination_pubkey: ElGamalPubkey = pod::ElGamalPubkey::zeroed().try_into().unwrap();
        let auditor_pubkey = ElGamalKeypair::new_rand().public;
        let withdraw_withheld_authority_pubkey = ElGamalKeypair::new_rand().public;

        let fee_data = TransferWithFeeData::new(
            transfer_amount,
            (spendable_balance, &spendable_ciphertext),
            &source_keypair,
            (&destination_pubkey, &auditor_pubkey),
            fee_parameters,
            &withdraw_withheld_authority_pubkey,
        )
        .unwrap();

        assert!(fee_data.verify().is_err());

        // auditor pubkey invalid
        let destination_pubkey: ElGamalPubkey = ElGamalKeypair::new_rand().public;
        let auditor_pubkey = pod::ElGamalPubkey::zeroed().try_into().unwrap();
        let withdraw_withheld_authority_pubkey = ElGamalKeypair::new_rand().public;

        let fee_data = TransferWithFeeData::new(
            transfer_amount,
            (spendable_balance, &spendable_ciphertext),
            &source_keypair,
            (&destination_pubkey, &auditor_pubkey),
            fee_parameters,
            &withdraw_withheld_authority_pubkey,
        )
        .unwrap();

        assert!(fee_data.verify().is_err());

        // withdraw authority invalid
        let destination_pubkey: ElGamalPubkey = ElGamalKeypair::new_rand().public;
        let auditor_pubkey = ElGamalKeypair::new_rand().public;
        let withdraw_withheld_authority_pubkey = pod::ElGamalPubkey::zeroed().try_into().unwrap();

        let fee_data = TransferWithFeeData::new(
            transfer_amount,
            (spendable_balance, &spendable_ciphertext),
            &source_keypair,
            (&destination_pubkey, &auditor_pubkey),
            fee_parameters,
            &withdraw_withheld_authority_pubkey,
        )
        .unwrap();

        assert!(fee_data.verify().is_err());
    }
}
