{
  description = "Operation experiment, pass three";
  inputs.base.url = "github:LiGoldragon/build-base";
  outputs = { self, base }: base.lib.workspaceOutputs {
    cargoNix = ./Cargo.nix;
    extraChecks = { check, ... }: { scope-ledger-plain = check "scope-ledger" []; operation-ledger-plain = check "operation-ledger" []; trial-ledger-plain = check "trial-ledger" []; trial-lock-plain = check "trial-lock" []; operation-scope-plain = check "operation-scope" []; };
    memberChecks = { check, ... }: {
      scope-ledger = check "scope-ledger" [ "trace" ];
      operation-scope = check "operation-scope" [ "trace" "datom" ];
      trial-ledger = check "trial-ledger" [ "trace" "datom" ];
      trial-lock = check "trial-lock" [ "trace" "datom" ];
      operation-ledger = check "operation-ledger" [ "trace" ];
    };
  };
}
