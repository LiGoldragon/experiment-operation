{
  description = "Operation experiment, pass two";
  inputs.base.url = "github:LiGoldragon/build-base";
  outputs = { self, base }: base.lib.workspaceOutputs {
    cargoNix = ./Cargo.nix;
    extraChecks = { check, ... }: { scope-ledger-plain = check "scope-ledger" []; operation-ledger-plain = check "operation-ledger" []; };
    memberChecks = { check, ... }: {
      scope-ledger = check "scope-ledger" [ "trace" ];
      operation-ledger = check "operation-ledger" [ "trace" ];
    };
  };
}
