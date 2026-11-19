{
  description = "Operation experiment, pass one";
  inputs.base.url = "github:LiGoldragon/build-base";
  outputs = { self, base }: base.lib.workspaceOutputs {
    cargoNix = ./Cargo.nix;
    extraChecks = { check, ... }: { scope-ledger-plain = check "scope-ledger" []; };
    memberChecks = { check, ... }: {
      scope-ledger = check "scope-ledger" [ "trace" ];
    };
  };
}
