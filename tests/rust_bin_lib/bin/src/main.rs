use lib::VALUE;

fn main() {
    println!("The value from lib is {VALUE:?}");
    println!(
        "The left-padded value is {:?}",
        left_pad::leftpad(VALUE, 20)
    );
}

#[cfg(test)]
mod tests {
    #[test]
    fn simple() {
        assert_eq!(1 + 1, 2);
    }
}
