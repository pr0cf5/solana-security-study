mod contracts;

#[cfg(test)]
mod tests {

    #[test]
    fn test_level0() {
        use crate::contracts::level0::main;
        main();
    }
}
