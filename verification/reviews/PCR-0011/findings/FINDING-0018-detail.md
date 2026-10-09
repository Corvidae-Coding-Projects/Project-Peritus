# FINDING-0018: The native ACL oracle rejected an equivalent symbolic rights mask

The native Windows test accepted only a hexadecimal SDDL rights mask, while `icacls /save` emitted the same exact access mask as symbolic rights. The final test-only parser accepts hexadecimal or the exact documented symbolic token set, computes the identical mask, and rejects missing or extra bits, unknown tokens, wrong principals or effects, inherited flags, and malformed ACEs. Production ACL behavior is unchanged.
