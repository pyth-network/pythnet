{ pkgs ? import <nixpkgs> {}
, ...
}:

with pkgs; mkShell {
  buildInputs = [
    clang_14
    curl
    go
    iconv
    llvmPackages_14.libclang
    nettle
    openssl_1_1
    pkg-config
    protobuf
    rustup
    systemd
    rocksdb_7_10
  ];

  shellHook = ''
    export LIBCLANG_PATH="${llvmPackages_14.libclang.lib}/lib";
  '';
}
