{
  description = "Operation experiment, pass one";
  inputs.base.url = "github:LiGoldragon/build-base";
  outputs = { self, base }: base.lib.workspaceOutputs {
    cargoNix = ./Cargo.nix;
    memberChecks = { check, ... }: {
      scope-ledger = check "scope-ledger" [ "trace" ];
      scope-ledger-plain = check "scope-ledger" [];
    };
  };
}
