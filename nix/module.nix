# NixOS module:
#
#   inputs.gmnb.url = "github:Go08er/GlazeMyNumbersBaby";
#   imports = [ inputs.gmnb.nixosModules.default ];
#   programs.gmnb.enable = true;
self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  cfg = config.programs.gmnb;
in
{
  options.programs.gmnb = {
    enable = lib.mkEnableOption "GMNB (GlazeMyNumbers,Baby), a pointlessly beautiful calculator";
    package = lib.mkOption {
      type = lib.types.package;
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.gmnb;
      defaultText = lib.literalExpression "gmnb.packages.\${system}.gmnb";
      description = "The GMNB package to install.";
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
  };
}
