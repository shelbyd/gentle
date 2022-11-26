use lib::VALUE;

fn main() {
    println!("The value from lib is {VALUE:?}");
}

#[cfg(test)]
mod tests {
    #[test]
    fn simple() {
        assert_eq!(1 + 1, 2);
    }
}
