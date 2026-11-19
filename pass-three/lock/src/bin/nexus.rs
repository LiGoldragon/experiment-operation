fn main() {
    operation_scope::ProcessRunning::<trial_lock::NexusDomain>::run(trial_lock::Nexus);
}
