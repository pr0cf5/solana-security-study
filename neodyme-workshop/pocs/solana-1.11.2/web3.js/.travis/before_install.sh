# |source| this file

if [[ -n $TRAVIS ]]; then
  wget -O - https://apt.llvm.org/llvm-snapshot.gpg.key | sudo apt-key add -
  sudo apt-add-repository "deb http://apt.llvm.org/bionic/ llvm-toolchain-bionic-10 main"
  sudo apt-get update
  sudo apt-get install -y clang-7 --allow-unauthenticated
  sudo apt-get install -y openssl --allow-unauthenticated
  sudo apt-get install -y libssl-dev --allow-unauthenticated
  sudo apt-get install -y libssl1.1 --allow-unauthenticated
  clang-7 --version
fi

sh -c "$(curl -sSfL https://release.solana.com/edge/install)"
PATH="$HOME/.local/share/solana/install/active_release/bin:$PATH"
solana --version

if [[ -n $GITHUB_ACTIONS ]]; then
  echo "$HOME/.local/share/solana/install/active_release/bin" >> "$GITHUB_PATH"
  npm install -g npm@7
fi
