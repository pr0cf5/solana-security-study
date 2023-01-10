const {
  Keypair,
  Transaction,
  LAMPORTS_PER_SOL,
  Connection,
  sendAndConfirmTransaction,
  BpfLoader,
  BPF_LOADER_PROGRAM_ID,
  PublicKey,
  SystemProgram,
} = require("@solana/web3.js");
const { Tip } = require("tip");
const fs = require("fs");

const connection = new Connection("http://localhost:8899", "confirmed");

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
  222, 53, 138, 189, 224, 216, 117, 173, 10, 149, 53, 45, 73, 251, 237, 246, 15,
  185, 186, 82, 177, 240, 148, 69, 241, 227, 167, 80, 141, 89, 240, 121, 121,
  35, 172, 247, 68, 251, 226, 218, 48, 63, 176, 109, 168, 89, 238, 135,
]);
let richBoy = Keypair.fromSecretKey(richBoySecretKey);

(async () => {
  console.log(`[*] authority: ${authority.publicKey}`);
  console.log(`[*] rich boy: ${richBoy.publicKey}`);

  if ((await connection.getBalance(authority.publicKey)) < LAMPORTS_PER_SOL) {
    const sig = await connection.requestAirdrop(
      authority.publicKey,
      100 * LAMPORTS_PER_SOL
    );
    await connection.confirmTransaction({ signature: sig });
    const balance = await connection.getBalance(authority.publicKey);
    console.log(
      `[+] airdrop on authority complete: ${balance / LAMPORTS_PER_SOL} SOL`
    );
  }
  if (
    (await connection.getBalance(richBoy.publicKey)) <
    43 * LAMPORTS_PER_SOL
  ) {
    const sig = await connection.requestAirdrop(
      richBoy.publicKey,
      100 * LAMPORTS_PER_SOL
    );
    await connection.confirmTransaction({ signature: sig });
    const balance = await connection.getBalance(authority.publicKey);
    console.log(
      `[+] airdrop on richBoy complete: ${balance / LAMPORTS_PER_SOL} SOL`
    );
  }

  // load wallet code
  const tipCode = fs.readFileSync("./level3.so");
  const tipProgram = Keypair.generate();
  const tipProgramId = tipProgram.publicKey;
  {
    const success = await BpfLoader.load(
      connection,
      authority,
      tipProgram,
      tipCode,
      BPF_LOADER_PROGRAM_ID
    );
    if (!success) {
      console.log("[!] Failed to load tip program!");
      throw "fail";
    } else {
      console.log("[+] Successfully loaded tip program");
    }
  }

  console.log(`[+] Tip programID: ${tipProgramId}`);

  // send init transaction to create vault
  var vaultAddress = "";
  {
    const [ix, vaultAddress_] = Tip.initialize(
      tipProgramId,
      authority.publicKey
    );
    vaultAddress = vaultAddress_;
    let tx = new Transaction().add(ix);
    await sendAndConfirmTransaction(connection, tx, [authority]);
    console.log(`[+] Initialization done, vault address at: ${vaultAddress}`);
  }

  const pool = Keypair.generate();
  const poolAddress = pool.publicKey;
  {
    const poolSize = 72;
    const ix = SystemProgram.createAccount({
      fromPubkey: authority.publicKey,
      newAccountPubkey: poolAddress,
      lamports: await connection.getMinimumBalanceForRentExemption(poolSize),
      space: poolSize,
      programId: tipProgramId,
    });
    let tx = new Transaction().add(ix);
    await sendAndConfirmTransaction(connection, tx, [authority, pool]);
  }

  // authority creates pool
  {
    const ix = Tip.createPool(
      tipProgramId,
      vaultAddress,
      authority.publicKey,
      poolAddress
    );
    let tx = new Transaction().add(ix);
    await sendAndConfirmTransaction(connection, tx, [authority]);
    console.log(`[+] Pool created at: ${poolAddress}`);
  }

  // richBoy makes donation
  {
    const ix = Tip.tip(
      tipProgramId,
      vaultAddress,
      poolAddress,
      richBoy.publicKey,
      42 * LAMPORTS_PER_SOL
    );
    let tx = new Transaction().add(ix);
    await sendAndConfirmTransaction(connection, tx, [richBoy]);
    let vaultBalance = await connection.getBalance(vaultAddress);
    console.log(
      `[+] RichBoy made donation, vault balance is ${
        vaultBalance / LAMPORTS_PER_SOL
      } SOL`
    );
  }
})();
