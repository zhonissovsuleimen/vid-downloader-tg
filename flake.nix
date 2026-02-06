{
  description = "vid-downloader-tg devshell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
  };

  outputs = { self, nixpkgs }:
  let
    system = "x86_64-linux";
    pkgs = import nixpkgs { inherit system; };
  in
  {
    devShells.${system}.default = pkgs.mkShell {
      packages = with pkgs; [
        rustc
        cargo
        rustfmt
        rust-analyzer

        chromium
        openssl
        ffmpeg
      ];

      env = {
        TELOXIDE_TOKEN="";
        RUST_BACKTRACE="1";
        OPENSSL_DIR="${pkgs.openssl.dev}";
        OPENSSL_LIB_DIR="${pkgs.openssl.out}/lib";
        OPENSSL_INCLUDE_DIR="${pkgs.openssl.dev}/include";
      };
    };
  };
}
