{ pkgs ? import <nixpkgs> {}
, ...
}:

with pkgs; mkShell {
  buildInputs = [
    clang
    curl
    go
    iconv
    llvmPackages.libclang
    nettle
    openssl_1_1
    pkg-config
    protobuf
    rustup
    systemd
  ];

  shellHook = ''
    export LIBCLANG_PATH="${llvmPackages.libclang.lib}/lib";
  '';
}
