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
  if (typeof x == "bigint") {
    let rv = new Array(8);
    let ba = new BigUint64Array(1);
    let buffer = new Uint8Array(ba.buffer);
    ba[0] = x;
    for (var i = 0; i < buffer.length; i++) {
      rv[i] = buffer[i];
    }
    return rv;
  } else {
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

function packFloat64(x) {
  let rv = new Array(8);
  let f64 = new Float64Array(1);
  let u8 = new Uint8Array(f64.buffer);
  f64[0] = x;
  for (var i = 0; i < u8.length; i++) {
    rv[i] = u8[i];
  }
  return rv;
}

function bytesToArray(b) {
  let a = new Array(b.length);
  for (var i = 0; i < b.length; i++) {
    a[i] = b[i];
  }
  return a;
}

function initialize(programId, initializerAddress) {
  const [vaultAddress, vaultSeed] = PublicKey.findProgramAddressSync(
    [],
    programId
  );
  if (
    PublicKey.createProgramAddressSync(
      [Buffer.from([vaultSeed])],
      programId
    ).toBase58() !== vaultAddress.toBase58()
  ) {
    throw "unreachable";
  }
  const vaultMeta = AccountMeta(vaultAddress, false);
  const initializerMeta = AccountMeta(initializerAddress, true);
  const rentMeta = AccountMeta(SYSVAR_RENT_PUBKEY, false);
  const spMeta = AccountMetaReadonly(SystemProgram.programId, false);
  const keys = [vaultMeta, initializerMeta, rentMeta, spMeta];
  let instruction = new TransactionInstruction({
    keys,
    programId,
    data: [0].concat(
      [vaultSeed].concat(
        packFloat64(0.0).concat(bytesToArray(initializerAddress.toBytes()))
      )
    ),
  });
  return [instruction, vaultAddress];
}

function createPool(programId, vaultAddress, authorityAddress, poolAddress) {
  const vaultMeta = AccountMeta(vaultAddress, false);
  const authorityMeta = AccountMetaReadonly(authorityAddress, true);
  const poolMeta = AccountMeta(poolAddress, false);
  const keys = [vaultMeta, authorityMeta, poolMeta];
  let instruction = new TransactionInstruction({
    keys,
    programId,
    data: [1],
  });
  return instruction;
}

function tip(programId, vaultAdress, poolAddress, sourceAddress, amount) {
  const vaultMeta = AccountMeta(vaultAdress, false);
  const poolMeta = AccountMeta(poolAddress, false);
  const sourceMeta = AccountMeta(sourceAddress, true);
  const spMeta = AccountMetaReadonly(SystemProgram.programId, false);
  const keys = [vaultMeta, poolMeta, sourceMeta, spMeta];
  let instruction = new TransactionInstruction({
    keys,
    programId: programId,
    data: [2].concat(packInt64(amount)),
  });
  return instruction;
}

function withdraw(
  programId,
  vaultAdress,
  poolAddress,
  withdrawerAddress,
  amount
) {
  const vaultMeta = AccountMeta(vaultAdress, false);
  const poolMeta = AccountMeta(poolAddress, false);
  const withdrawerMeta = AccountMeta(withdrawerAddress, true);
  const spMeta = AccountMetaReadonly(SystemProgram.programId, false);
  const keys = [vaultMeta, poolMeta, withdrawerMeta, spMeta];
  let instruction = new TransactionInstruction({
    keys,
    programId: programId,
    data: [3].concat(packInt64(amount)),
  });
  return instruction;
}

exports.Tip = {
  initialize,
  createPool,
  tip,
  withdraw,
};
