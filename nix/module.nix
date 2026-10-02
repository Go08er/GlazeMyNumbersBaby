# NixOS module:
#
#   inputs.gmnb.url = "github:Go08er/GlazeMyNumbersBaby";
#   imports = [ inputs.gmnb.nixosModules.default ];
#   programs.gmnb.enable = true;    # the glazed twin
#   programs.dgmnb.enable = true;   # the lean twin
self:
{
  config,
  lib,
  pkgs,
  ...
}:
let
  packages = self.packages.${pkgs.stdenv.hostPlatform.system};
in
{
  options.programs.gmnb = {
    enable = lib.mkEnableOption "GMNB (GlazeMyNumbers,Baby), a pointlessly beautiful calculator";
    package = lib.mkOption {
      type = lib.types.package;
      default = packages.gmnb;
      defaultText = lib.literalExpression "gmnb.packages.\${system}.gmnb";
      description = "The GMNB package to install.";
    };
  };

  options.programs.dgmnb = {
    enable = lib.mkEnableOption "DGMNB (Don't Glaze My Numbers, Baby), the lean twin of GMNB";
    package = lib.mkOption {
      type = lib.types.package;
      default = packages.dgmnb;
      defaultText = lib.literalExpression "gmnb.packages.\${system}.dgmnb";
      description = "The DGMNB package to install.";
    };
  };

  config.environment.systemPackages =
    lib.optional config.programs.gmnb.enable config.programs.gmnb.package
    ++ lib.optional config.programs.dgmnb.enable config.programs.dgmnb.package;
}
