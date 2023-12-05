use ethers::abi::Uint;
use ethers::contract::EthCall;

/// For decoding and displaying `Panic(uint256)` errors.
///
/// EthError derived decode_with_selector is buggy, so we derive `EthCall` instead. Decode with `AbiDecode::decode`.
#[derive(EthCall)]
pub struct Panic(Uint);

impl std::fmt::Display for Panic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let e = PanicError::from_code(self.0.low_u32());
        write!(f, "Panic code: {:#x}, {e}", self.0)
    }
}

enum PanicError {
    Generic,
    AssertFailed,
    ArithmeticOverflow = 0x11,
    DivisionByZero,
    InvalidEnumConversion = 0x21,
    InvalidEncoding,
    EmptyArrayPop = 0x31,
    OutOfBoundsAccess,
    ExcessiveAllocation = 0x41,
    UninitializedInternalFunction = 0x51,
    Unknown,
}

impl PanicError {
    fn from_code(code: u32) -> Self {
        match code {
            0 => PanicError::Generic,
            0x01 => PanicError::AssertFailed,
            0x11 => PanicError::ArithmeticOverflow,
            0x12 => PanicError::DivisionByZero,
            0x21 => PanicError::InvalidEnumConversion,
            0x22 => PanicError::InvalidEncoding,
            0x31 => PanicError::EmptyArrayPop,
            0x32 => PanicError::OutOfBoundsAccess,
            0x41 => PanicError::ExcessiveAllocation,
            0x51 => PanicError::UninitializedInternalFunction,
            _ => PanicError::Unknown,
        }
    }
}

impl std::fmt::Display for PanicError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let desc = match self {
            PanicError::Generic => "Generic compiler inserted panic",
            PanicError::AssertFailed => "Assertion failed",
            PanicError::ArithmeticOverflow => "Arithmetic operation resulted in overflow",
            PanicError::DivisionByZero => "Division or modulo by zero",
            PanicError::InvalidEnumConversion => "Invalid enum conversion",
            PanicError::InvalidEncoding => "Invalid encoding",
            PanicError::EmptyArrayPop => "Attempted to pop an empty array",
            PanicError::OutOfBoundsAccess => "Out-of-bounds access",
            PanicError::ExcessiveAllocation => "Excessive memory allocation",
            PanicError::UninitializedInternalFunction => {
                "Called an uninitialized internal function"
            }
            PanicError::Unknown => "Unknown panic",
        };
        write!(f, "{desc}")
    }
}

#[cfg(test)]
mod test {
    use ethers::{abi::AbiDecode, contract::EthError};

    use super::Panic;

    fn parse_abi_err_data(err: &str) -> String {
        let revert_data = hex::decode(
            err.strip_prefix("Contract call reverted with data: 0x")
                .unwrap(),
        )
        .unwrap();
        if let Ok(p) = Panic::decode(&revert_data) {
            p.to_string()
        } else if let Some(s) = String::decode_with_selector(&revert_data) {
            s
        } else {
            panic!("failed to decode")
        }
    }

    #[test]
    fn test_sol_revert() {
        let err_string = "Contract call reverted with data: 0x08c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001c74657374206661696c656420746f2063726561746520636c69656e7400000000";
        let err = parse_abi_err_data(err_string);
        assert_eq!(err, "test failed to create client");
    }

    #[test]
    fn test_sol_panic() {
        let err_string = "Contract call reverted with data: 0x4e487b710000000000000000000000000000000000000000000000000000000000000012";
        let err = parse_abi_err_data(err_string);
        assert_eq!(err, "Panic code: 0x12, Division or modulo by zero");
    }
}
