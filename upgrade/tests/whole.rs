use experiment_upgrade::{GeneratingNexus, NexusSchema};
fn schema() -> NexusSchema {
    NexusSchema {
        signal: "Signal [] [ Move ] [ Moved Refused ] []".into(),
        memory: "Memory [] [ Record.{ Key Count } ] [ Increment.Key ] [ Incremented.Count Refused.[ Missing ] ] [ Key.String Count.Integer ]".into(),
        operation: "Operation [] [ Move.{ Increment Moved } ] [] [] [ Move.{ Key } Moved.{ Count } ]".into(),
        translation: "Translation [] [] [] []".into(),
    }
}
#[test]
fn whole_attempt_wires_leaf_yield_and_output_by_type() {
    let generated = schema().generate_nexus().unwrap();
    assert!(
        generated
            .bindings
            .contains("Operable<NexusDomain> for Move")
    );
    assert!(generated.bindings.contains("scope.run(Increment"));
    assert!(
        generated
            .bindings
            .contains("operation_scope::Emitting::emit(")
    );
    assert!(
        generated
            .bindings
            .contains("r#count:_position_0.r#count.clone()")
    );
}
#[test]
fn missing_type_refuses_before_rust() {
    let mut schema = schema();
    schema.operation = schema.operation.replace("Move.{ Key }", "Move.{ Count }");
    let error = schema.generate_nexus().err().unwrap().to_string();
    assert!(error.contains("Move: type Key held 0 times"), "{error}");
}
#[test]
fn duplicate_type_refuses_before_rust() {
    let mut schema = schema();
    schema.operation = schema.operation.replace("Move.{ Key }", "Move.{ Key Key }");
    let error = schema.generate_nexus().err().unwrap().to_string();
    assert!(error.contains("Move: type Key held 2 times"), "{error}");
}
#[test]
fn whole_change_runs_smaller_changes_on_the_same_transaction() {
    let mut schema = schema();
    schema.memory="Memory [] [ Record.{ Key Count } ] [ Move.{ Increment Moved } Increment.Key ] [ Moved.{ Count } Incremented.Count Refused.[ Missing ] ] [ Key.String Count.Integer Move.{ Key } ]".into();
    schema.operation = "Operation [] [] [] []".into();
    let generated = schema.generate_nexus().unwrap();
    assert!(
        generated
            .bindings
            .contains("Changeable<NexusDomain> for Move")
    );
    assert!(generated.bindings.contains("memory.run(Increment"));
    assert!(!generated.bindings.contains("scope.run(Increment"));
}
#[test]
fn early_output_is_retained_after_a_tail() {
    let mut schema = schema();
    schema.operation="Operation [] [ Move.{ Increment Moved Tail } ] [ Tail ] [ TailDone ] [ Move.{ Key } Moved.{ Count } ]".into();
    let generated = schema.generate_nexus().unwrap();
    let implementation = generated
        .bindings
        .split("impl operation_scope::Operable")
        .nth(1)
        .unwrap();
    let emitted = implementation
        .find("operation_scope::Emitting::emit(")
        .unwrap();
    let tail = implementation.find("scope.run(Tail").unwrap();
    assert!(emitted < tail);
    assert!(implementation.contains("Ok(_position_1.clone())"));
}

#[test]
fn explicit_branches_emit_the_selected_output_and_propagate_same_name_refusal() {
    let mut schema = schema();
    schema.operation="Operation [] [ Move.{ Decide.[ Yes.{ Moved } No.{ Moved } ] } ] [ Decide.Key ] [ Decision.[ Yes.Count No.Count Missing ] ] [ Move.{ Key } Moved.{ Count } ]".into();
    let generated = schema.generate_nexus().unwrap();
    assert!(generated.bindings.contains("Decision::Yes(branch_value)"));
    assert!(generated.bindings.contains("Decision::No(branch_value)"));
    assert!(
        generated
            .bindings
            .contains("Decision::Missing=>return Err(Refused::Missing.into())")
    );
    assert!(generated.bindings.contains("Ok(_branch_1.clone())"));
}

#[test]
fn value_already_held_can_choose_a_branch() {
    let mut schema = schema();
    schema.operation="Operation [] [ Move.{ Decision.[ Yes.{ Moved } No.{ Moved } ] } ] [] [] [ Move.{ Decision } Decision.[ Yes.Count No.Count ] Moved.{ Count } ]".into();
    let generated = schema.generate_nexus().unwrap();
    assert!(generated.bindings.contains("match _position_0"));
    assert!(!generated.bindings.contains("scope.run(Decision"));
}

#[test]
fn unhandled_variant_requires_a_same_name_domain_refusal() {
    let mut schema = schema();
    schema.operation="Operation [] [ Move.{ Decide Moved } ] [ Decide.Key ] [ Decision.[ Yes.Count Unknown ] ] [ Move.{ Key } Moved.{ Count } ]".into();
    let error = schema.generate_nexus().err().unwrap().to_string();
    assert!(
        error.contains("Move: unhandled Decision::Unknown"),
        "{error}"
    );
}
