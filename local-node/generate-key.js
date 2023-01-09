const {Keypair} = require("@solana/web3.js");

var wallet = Keypair.generate();
console.log(`[+] Public Key: ${wallet.publicKey}`);
console.log(`[+] Secret Key: ${JSON.stringify(wallet.secretKey.toString())}`);
