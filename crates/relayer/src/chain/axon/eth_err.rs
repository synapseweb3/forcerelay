use ethers::abi::ethabi::{decode, ParamType};
use hex::FromHex;

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

fn parse_panic_error(hex_string: &str) -> PanicError {
    match hex_string {
        "00" => PanicError::Generic,
        "01" => PanicError::AssertFailed,
        "11" => PanicError::ArithmeticOverflow,
        "12" => PanicError::DivisionByZero,
        "21" => PanicError::InvalidEnumConversion,
        "22" => PanicError::InvalidEncoding,
        "31" => PanicError::EmptyArrayPop,
        "32" => PanicError::OutOfBoundsAccess,
        "41" => PanicError::ExcessiveAllocation,
        "51" => PanicError::UninitializedInternalFunction,
        _ => PanicError::Unknown,
    }
}

fn handle_panic_error(error: PanicError) -> String {
    match error {
        PanicError::Generic => format!(
            "Panic code: 0x{:x}, Generic compiler inserted panic",
            error as u16
        ),
        PanicError::AssertFailed => format!("Panic code: 0x{:x}, Assertion failed", error as u16),
        PanicError::ArithmeticOverflow => format!(
            "Panic code: 0x{:x}, Arithmetic operation resulted in overflow",
            error as u16
        ),
        PanicError::DivisionByZero => {
            format!(
                "Panic code: 0x{:x}, Division or modulo by zero",
                error as u16
            )
        }
        PanicError::InvalidEnumConversion => {
            format!("Panic code: 0x{:x}, Invalid enum conversion", error as u16)
        }
        PanicError::InvalidEncoding => {
            format!("Panic code: 0x{:x}, Invalid encoding", error as u16)
        }
        PanicError::EmptyArrayPop => format!(
            "Panic code: 0x{:x}, Attempted to pop an empty array",
            error as u16
        ),
        PanicError::OutOfBoundsAccess => {
            format!("Panic code: 0x{:x}, Out-of-bounds access", error as u16)
        }
        PanicError::ExcessiveAllocation => format!(
            "Panic code: 0x{:x}, Excessive memory allocation",
            error as u16
        ),
        PanicError::UninitializedInternalFunction => {
            format!(
                "Panic code: 0x{:x}, Called an uninitialized internal function",
                error as u16
            )
        }
        PanicError::Unknown => format!("Panic code: 0x{:x}, Unknown panic", error as u16),
    }
}

const ERROR_SELECTOR: &str = "08c379a0";
const PANIC_SELECTOR: &str = "4e487b71";

#[derive(PartialEq)]
enum AbiErrorType {
    Panic,
    Revert,
    Unknown,
}

impl AbiErrorType {
    fn from_signature(signature: &str) -> Self {
        match signature {
            PANIC_SELECTOR => AbiErrorType::Panic,
            ERROR_SELECTOR => AbiErrorType::Revert,
            _ => AbiErrorType::Unknown,
        }
    }

    fn param_type(&self) -> Option<ParamType> {
        match self {
            AbiErrorType::Panic => Some(ParamType::Uint(32)),
            AbiErrorType::Revert => Some(ParamType::String),
            AbiErrorType::Unknown => None,
        }
    }
}

pub(crate) fn parse_abi_err_data(err: &str) -> String {
    let start_index = "Contract call reverted with data: 0x".len();
    let method_signature_len = "08c379a0".len();

    let method_signature = &err[start_index..(start_index + method_signature_len)];
    let hex_error_message = &err[(start_index + method_signature_len)..];

    let hex_error: Vec<u8> = match FromHex::from_hex(hex_error_message) {
        Ok(hex_error) => hex_error,
        Err(err) => {
            return format!(
                "parse_abi_err_data from_hex: {:?}, data: {}!",
                err, hex_error_message
            );
        }
    };

    let error_type = AbiErrorType::from_signature(method_signature);
    if let Some(error_abi) = error_type.param_type() {
        let decoded = decode(&[error_abi], &hex_error);
        match decoded {
            Ok(decoded) => {
                let strings: Vec<String> =
                    decoded.into_iter().map(|token| token.to_string()).collect();
                let result = strings.join(";");
                if error_type == AbiErrorType::Panic {
                    let panic_error = parse_panic_error(&result);
                    handle_panic_error(panic_error)
                } else {
                    result
                }
            }
            Err(err) => {
                format!(
                    "parse_abi_err_data decode: {:?}, data: {:x?}!",
                    err, hex_error
                )
            }
        }
    } else {
        format!(
            "parse_abi_err_data: unknown exception method {}!",
            method_signature
        )
    }
}

#[cfg(test)]
mod test {
    use crate::chain::axon::eth_err::parse_abi_err_data;

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

    #[test]
    fn test_sol_unknown_method() {
        let err_string = "Contract call reverted with data: 0x18c379a00000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001c74657374206661696c656420746f2063726561746520636c69656e7400000000";
        let err = parse_abi_err_data(err_string);
        assert_eq!(
            err,
            "parse_abi_err_data: unknown exception method 18c379a0!"
        );
    }

    #[test]
    fn test_sol_invalid_hex() {
        let err_string = "Contract call reverted with data: 0x08c379a00x00000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001c74657374206661696c656420746f2063726561746520636c69656e7400000000";
        let err = parse_abi_err_data(err_string);
        assert_eq!(err, "parse_abi_err_data from_hex: InvalidHexCharacter { c: 'x', index: 1 }, data: 0x00000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001c74657374206661696c656420746f2063726561746520636c69656e7400000000!");
    }

    #[test]
    fn test_sol_invalid_data() {
        let err_string = "Contract call reverted with data: 0x08c379a00000000000000000000000000000000000000000000000000000000000000050000000000000000000000000000000000000000000000000000000000000001c74657374206661696c656420746f2063726561746520636c69656e7400000000";
        let err = parse_abi_err_data(err_string);
        assert_eq!(err, "parse_abi_err_data decode: InvalidData, data: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 50, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1c, 74, 65, 73, 74, 20, 66, 61, 69, 6c, 65, 64, 20, 74, 6f, 20, 63, 72, 65, 61, 74, 65, 20, 63, 6c, 69, 65, 6e, 74, 0, 0, 0, 0]!");
    }
}
