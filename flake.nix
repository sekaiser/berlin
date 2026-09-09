{
  description = "berlin nixshell";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = {
    nixpkgs,
    flake-utils,
    ...
  }:
    flake-utils.lib.eachDefaultSystem (
      system: let
        pkgs = import nixpkgs {inherit system;};
        emacsWithOxHugo = pkgs.emacs.pkgs.withPackages (epkgs: [epkgs.ox-hugo]);
        graphicsTex = pkgs.texliveBasic.withPackages (tex: [tex.standalone tex.preview tex.pgf tex.pgfplots tex.dvisvgm tex.amsfonts tex.luatex tex.luaotfload tex.fontspec tex.lm tex.unicode-data tex.luatex85 tex.luatexbase tex.lualibs]);
      in {
        formatter = pkgs.alejandra;

        devShells.default = pkgs.mkShell {
          packages = [
            pkgs.rustc
            pkgs.cargo
            pkgs.clippy
            pkgs.rust-analyzer
            pkgs.rustfmt
            emacsWithOxHugo
            pkgs.nodejs_24
            pkgs.gitMinimal
            pkgs.nil
          ];
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
        };

        # Author-time TikZ compilation is deliberately absent from the build shell.
        devShells.diagrams = pkgs.mkShell {
          packages = [
            emacsWithOxHugo
            graphicsTex
            pkgs.poppler-utils
          ];
        };

        # Optional, explicitly authorized paper compilation and vector figures.
        devShells.papers = pkgs.mkShell {
          packages = [graphicsTex pkgs.pandoc pkgs.poppler-utils pkgs.rustc pkgs.cargo];
        };
      }
    );
}
