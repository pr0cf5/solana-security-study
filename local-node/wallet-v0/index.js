const {
    TransactionInstruction,
    PublicKey,
    SYSVAR_RENT_PUBKEY,
    SystemProgram,
} = require("@solana/web3.js");

const lo = require('@solana/buffer-layout');

const WALLET_LEN = 64;
const WALLET = lo.struct([
    lo.seq(lo.u8(), 32, "authority"),
    lo.seq(lo.u8(), 32, "vault"),
])

function AccountMetaReadonly(pubkey, isSigner) {
    return {
        pubkey,
        isWritable: false,
        isSigner,
    }
}

function AccountMeta(pubkey, isSigner) {
    return {
        pubkey,
        isWritable: true,
        isSigner,
    }
}

function packInt64(x) {
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


function walletInitialize(walletProgramId, authority) {
    const [walletAddress, walletBumpSeed] = PublicKey.findProgramAddressSync([authority.publicKey.toBytes()], walletProgramId);
    const walletMeta = AccountMeta(walletAddress, false);
    const [vaultAddress, vaultBumpSeed] = PublicKey.findProgramAddressSync([authority.publicKey.toBytes(), Buffer.from("VAULT")], walletProgramId);
    const vaultMeta = AccountMeta(vaultAddress, false);
    const authorityMeta = AccountMeta(authority.publicKey, true);
    const sysvarRentMeta = AccountMeta(SYSVAR_RENT_PUBKEY, false);
    const systemProgramMeta = AccountMeta(SystemProgram.programId, false);
    const keys = [
        walletMeta, vaultMeta, authorityMeta, sysvarRentMeta, systemProgramMeta
    ];
    let instruction = new TransactionInstruction({
        keys,
        programId: walletProgramId,
        data: [0],
    });
    return instruction;
}

function walletDeposit(walletProgramId, authority, source, amount) {
    const [walletAddress, walletBumpSeed] = PublicKey.findProgramAddressSync([authority.publicKey.toBytes()], walletProgramId);
    const walletMeta = AccountMeta(walletAddress, false);
    const [vaultAddress, vaultBumpSeed] = PublicKey.findProgramAddressSync([authority.publicKey.toBytes(), Buffer.from("VAULT")], walletProgramId);
    const vaultMeta = AccountMeta(vaultAddress, false);
    const sourceMeta = AccountMeta(source, true);
    const systemProgramMeta = AccountMetaReadonly(SystemProgram.programId, false);
    const keys = [
        walletMeta, vaultMeta, sourceMeta, systemProgramMeta
    ];
    let instruction = new TransactionInstruction({
        keys,
        programId: walletProgramId,
        data: [1].concat(packInt64(amount)),
    });
    return instruction;
}

function walletWithdraw(walletProgramId, walletAddress, vaultAddress, authority, dest, amount) {
    const walletMeta = AccountMeta(walletAddress, false);
    const vaultMeta = AccountMeta(vaultAddress, false);
    const authorityMeta = AccountMeta(authority.publicKey, true);
    const destMeta = AccountMeta(dest, true);
    const keys = [
        walletMeta, vaultMeta, authorityMeta, destMeta
    ];
    let instruction = new TransactionInstruction({
        keys,
        programId: walletProgramId,
        data: [2].concat(packInt64(amount)),
    });
    return instruction;
}

function decodeWalletData(data) {
    const wallet = WALLET.decode(data);
    return [new PublicKey(wallet.authority), new PublicKey(wallet.vault)];
}

exports.Wallet = {
    initialize: walletInitialize,
    deposit: walletDeposit,
    withdraw: walletWithdraw,
    decodeWalletData,
    WALLET_LEN: WALLET_LEN,
}

