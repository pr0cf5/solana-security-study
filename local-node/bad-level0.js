const {
    Keypair,
    Transaction,
    LAMPORTS_PER_SOL,
    Connection,
    sendAndConfirmTransaction,
    BpfLoader,
    BPF_LOADER_PROGRAM_ID,
    PublicKey
} = require("@solana/web3.js");
const { Wallet } = require("wallet-v0");
const fs = require('fs');

const connection = new Connection("http://localhost:8899", "confirmed");

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

// initialize authority's keys
let authoritySecretKey = Uint8Array.from([
    202, 171, 192, 129, 150, 189, 204, 241, 142, 71, 205, 2, 81, 97, 2, 176, 48,
    81, 45, 1, 96, 138, 220, 132, 231, 131, 120, 77, 66, 40, 97, 172, 91, 245, 84,
    221, 157, 190, 9, 145, 176, 130, 25, 43, 72, 107, 190, 229, 75, 88, 191, 136,
    7, 167, 109, 91, 170, 164, 186, 15, 142, 36, 12, 23,
]);
let authority = Keypair.fromSecretKey(authoritySecretKey);

let richBoySecretKey = Uint8Array.from([
    174, 47, 154, 16, 202, 193, 206, 113, 199, 190, 53, 133, 169, 175, 31, 56,
    222, 53, 138, 189, 224, 216, 117, 173, 10, 149, 53, 45, 73, 251, 237, 246,
    15, 185, 186, 82, 177, 240, 148, 69, 241, 227, 167, 80, 141, 89, 240, 121,
    121, 35, 172, 247, 68, 251, 226, 218, 48, 63, 176, 109, 168, 89, 238, 135,
  ]);
let richBoy = Keypair.fromSecretKey(richBoySecretKey);

(async () => {

    console.log(`[*] authority: ${authority.publicKey}`);
    console.log(`[*] rich boy: ${richBoy.publicKey}`);

    if ( (await connection.getBalance(authority.publicKey)) < LAMPORTS_PER_SOL )
    {
        const sig = await connection.requestAirdrop(authority.publicKey, 100 * LAMPORTS_PER_SOL);
        await connection.confirmTransaction( { signature: sig });
        const balance = await connection.getBalance(authority.publicKey);
        console.log(`[+] airdrop on authority complete: ${balance / LAMPORTS_PER_SOL} SOL`);
    }
    if ( (await connection.getBalance(richBoy.publicKey)) < 43 * LAMPORTS_PER_SOL )
    {
        const sig = await connection.requestAirdrop(richBoy.publicKey, 100 * LAMPORTS_PER_SOL);
        await connection.confirmTransaction( { signature: sig });
        const balance = await connection.getBalance(authority.publicKey);
        console.log(`[+] airdrop on richBoy complete: ${balance / LAMPORTS_PER_SOL} SOL`);
    }

    // load wallet code
    const walletCode = fs.readFileSync("./level0-bad.so");
    const walletProgram = Keypair.generate();
    const walletProgramId = walletProgram.publicKey;
    {
        const success = await BpfLoader.load(connection, authority, walletProgram, walletCode, BPF_LOADER_PROGRAM_ID);
        if (!success) {
            console.log("[!] Failed to load wallet program!");
            throw "fail";
        }
        else {
            console.log("[+] Successfully loaded wallet program");
        }
    }

    console.log(`[+] Wallet programID: ${walletProgramId}`);

    // send init transaction to create vault
    {
        const ix = Wallet.initialize(walletProgramId, authority.publicKey);
        let tx = new Transaction().add(ix);
        await sendAndConfirmTransaction(connection, tx, [authority]);
        console.log("[+] Initialization done");
    }

    // rich boy deposits funds to the vault
    var walletAddress = "";
    var vaultAddress = "";
    {
        const ix = Wallet.deposit(walletProgramId, authority.publicKey, richBoy.publicKey, 42 * LAMPORTS_PER_SOL);
        let tx = new Transaction().add(ix);
        await sendAndConfirmTransaction(connection, tx, [richBoy]);
        const [walletAddress_, _walletBumpSeed] = PublicKey.findProgramAddressSync([authority.publicKey.toBytes()], walletProgramId);
        walletAddress = walletAddress_;
        const accountInfo = await connection.getAccountInfo(walletAddress);
        const [_, vaultAddress_] = Wallet.decodeWalletData(accountInfo.data);
        vaultAddress = vaultAddress_;
        const vaultBalance = await connection.getBalance(vaultAddress);
        console.log(`[+] RichBoy deposited funds, vault balance is ${vaultBalance / LAMPORTS_PER_SOL} SOL`);
    }

    console.log(`[+] Wallet: ${walletAddress}`);
    console.log(`[+] Vault: ${vaultAddress}`);

    {
        const ix = Wallet.withdraw(walletProgramId, walletAddress, vaultAddress, authority.publicKey, richBoy.publicKey, LAMPORTS_PER_SOL);
        let tx = new Transaction().add(ix);
        await sendAndConfirmTransaction(connection, tx, [authority, richBoy]);
    }
})();