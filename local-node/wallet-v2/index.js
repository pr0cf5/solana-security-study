const {
  TransactionInstruction,
  PublicKey,
  SYSVAR_RENT_PUBKEY,
  SystemProgram,
  Account,
} = require("@solana/web3.js");

const lo = require("@solana/buffer-layout");

const WALLET_LEN = 64;
const WALLET = lo.struct([
  lo.seq(lo.u8(), 32, "authority"),
  lo.seq(lo.u8(), 32, "vault"),
]);

function AccountMetaReadonly(pubkey, isSigner) {
  return {
    pubkey,
    isWritable: false,
    isSigner,
  };
}

function AccountMeta(pubkey, isSigner) {
  return {
    pubkey,
    isWritable: true,
    isSigner,
  };
}

function packInt64(x) {
  if (typeof (x) == "bigint") {
    let rv = new Array(8);
    let ba = new BigUint64Array(1);
    let buffer = new Uint8Array(ba.buffer);
    ba[0] = x;
    for (var i = 0; i < buffer.length; i++) {
      rv[i] = buffer[i];
    }
    return rv;
  }
  else {
    let rv = new Array(8);
    const U64 = lo.nu64be();
    let buffer = new Uint8Array(8);
    U64.encode(x, buffer);
    for (var i = 0; i < buffer.length; i++) {
      rv[i] = buffer[i];
    }
    rv.reverse();
    return rv;
  }
}

function walletInitialize(walletProgramId, authorityAddress) {
  const [walletAddress, _] = PublicKey.findProgramAddressSync(
    [authorityAddress.toBytes()],
    walletProgramId
  );
  const walletMeta = AccountMeta(walletAddress, false);
  const authorityMeta = AccountMeta(authorityAddress, true);
  const sysvarRentMeta = AccountMeta(SYSVAR_RENT_PUBKEY, false);
  const systemProgramMeta = AccountMeta(SystemProgram.programId, false);
  const keys = [walletMeta, authorityMeta, sysvarRentMeta, systemProgramMeta];
  let instruction = new TransactionInstruction({
    keys,
    programId: walletProgramId,
    data: [0],
  });
  return [instruction, walletAddress];
}

function walletDeposit(walletProgramId, authorityAddress, source, amount) {
  const [walletAddress, walletBumpSeed] = PublicKey.findProgramAddressSync(
    [authorityAddress.toBytes()],
    walletProgramId
  );
  const walletMeta = AccountMeta(walletAddress, false);
  const sourceMeta = AccountMeta(source, true);
  const systemProgramMeta = AccountMetaReadonly(SystemProgram.programId, false);
  const keys = [walletMeta, sourceMeta, systemProgramMeta];
  let instruction = new TransactionInstruction({
    keys,
    programId: walletProgramId,
    data: [1].concat(packInt64(amount)),
  });
  return instruction;
}

function walletWithdraw(
  walletProgramId,
  walletAddress,
  authorityAddress,
  dest,
  amount
) {
  const walletMeta = AccountMeta(walletAddress, false);
  const authorityMeta = AccountMeta(authorityAddress, true);
  const destMeta = AccountMeta(dest, false);
  const rentMeta = AccountMetaReadonly(SYSVAR_RENT_PUBKEY, false);
  const keys = [walletMeta, authorityMeta, destMeta, rentMeta];
  let instruction = new TransactionInstruction({
    keys,
    programId: walletProgramId,
    data: [2].concat(packInt64(amount)),
  });
  return instruction;
}

exports.Wallet = {
  initialize: walletInitialize,
  deposit: walletDeposit,
  withdraw: walletWithdraw,
  WALLET_LEN: WALLET_LEN,
};
