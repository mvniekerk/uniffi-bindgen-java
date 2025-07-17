uniffi::setup_scaffolding!();

#[uniffi::export]
pub fn say_true_or_not(true_or_not: bool) -> bool {
    println!("say_true_or_not: {}", true_or_not);
    true_or_not
}