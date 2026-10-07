/// <https://w3c.github.io/webcrypto/#sha-operations-digest>
pub fn digest(name: &str, message: &[u8]) -> Vec<u8> {
    // If the name member of normalizedAlgorithm is a case-sensitive string match for "SHA-1":
    //     Let result be the result of performing the SHA-1 hash function defined in Section 6.1
    //     of [FIPS-180-4] using message as the input message, M.
    // If the name member of normalizedAlgorithm is a case-sensitive string match for "SHA-256":
    //     Let result be the result of performing the SHA-256 hash function defined in Section 6.2
    //     of [FIPS-180-4] using message as the input message, M.
    // If the name member of normalizedAlgorithm is a case-sensitive string match for "SHA-384":
    //     Let result be the result of performing the SHA-384 hash function defined in Section 6.5
    //     of [FIPS-180-4] using message as the input message, M.
    // If the name member of normalizedAlgorithm is a case-sensitive string match for "SHA-512":
    //     Let result be the result of performing the SHA-512 hash function defined in Section 6.4
    //     of [FIPS-180-4] using message as the input message, M.
    // If performing the operation results in an error, then throw an OperationError.
    // Return result as a new byte sequence for the caller of digest.
    hash(name, message)
}
