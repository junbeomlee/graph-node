extern crate parity_wasm;

use ethabi::Token;
use wasmi::{
    self, nan_preserving_float::F32, ImportsBuilder, MemoryRef, ModuleImportResolver,
    ModuleInstance, ModuleRef, NopExternals, RuntimeValue, Signature,
};

use graph::prelude::BigInt;
use graph::web3::types::{H160, U256};

use super::class::*;
use super::{AscHeap, AscPtr};

struct TestModule {
    module: ModuleRef,
    memory: MemoryRef,
}

struct DummyImportResolver;

impl ModuleImportResolver for DummyImportResolver {
    fn resolve_func(
        &self,
        field_name: &str,
        _signature: &Signature,
    ) -> Result<wasmi::FuncRef, wasmi::Error> {
        Ok(match field_name {
            "abort" => wasmi::FuncInstance::alloc_host(
                Signature::new(
                    &[
                        wasmi::ValueType::I32,
                        wasmi::ValueType::I32,
                        wasmi::ValueType::I32,
                        wasmi::ValueType::I32,
                    ][..],
                    None,
                ),
                0,
            ),
            _ => panic!("requested non-existing import {}", field_name),
        })
    }
}

impl TestModule {
    fn new(path: &str) -> Self {
        let mut imports = ImportsBuilder::new();
        imports.push_resolver("env", &DummyImportResolver);

        // Load .wasm file into Wasmi interpreter
        let module = &wasmi::Module::from_parity_wasm_module(
            parity_wasm::deserialize_file(path).expect("Failed to deserialize wasm"),
        )
        .expect("Invalid module");

        let module_instance = ModuleInstance::new(module, &imports)
            .expect("Failed to instantiate module")
            .run_start(&mut NopExternals)
            .expect("Failed to start module");

        // Access the wasm runtime linear memory
        let memory = module_instance
            .export_by_name("memory")
            .expect("Failed to find memory export in the wasm module")
            .as_memory()
            .expect("Extern value is not Memory")
            .clone();

        Self {
            module: module_instance,
            memory,
        }
    }

    fn takes_ptr_returns_ptr<T, U>(&self, fn_name: &str, arg: AscPtr<T>) -> AscPtr<U> {
        self.module
            .invoke_export(fn_name, &[RuntimeValue::from(arg)], &mut NopExternals)
            .expect("call failed")
            .expect("call returned nothing")
            .try_into()
            .expect("call did not return pointer")
    }

    fn takes_ptr_ptr_returns_ptr<T, U, R>(
        &self,
        fn_name: &str,
        arg1: AscPtr<T>,
        arg2: AscPtr<U>,
    ) -> AscPtr<R> {
        self.module
            .invoke_export(
                fn_name,
                &[RuntimeValue::from(arg1), RuntimeValue::from(arg2)],
                &mut NopExternals,
            )
            .expect("call failed")
            .expect("call returned nothing")
            .try_into()
            .expect("call did not return pointer")
    }

    fn takes_val_returns_ptr<T>(&self, fn_name: &str, val: RuntimeValue) -> AscPtr<T> {
        self.module
            .invoke_export(fn_name, &[val], &mut NopExternals)
            .expect("call failed")
            .expect("call returned nothing")
            .try_into()
            .expect("call did not return pointer")
    }
}

impl AscHeap for TestModule {
    fn raw_new(&self, bytes: &[u8]) -> Result<u32, wasmi::Error> {
        let addr = self
            .module
            .invoke_export(
                "memory.allocate",
                &[RuntimeValue::I32(bytes.len() as i32)],
                &mut NopExternals,
            )
            .expect("call failed")
            .expect("call returned nothing")
            .try_into::<u32>()
            .expect("call did not return u32");

        self.memory.set(addr, bytes)?;
        Ok(addr)
    }

    fn get(&self, offset: u32, size: u32) -> Result<Vec<u8>, wasmi::Error> {
        self.memory.get(offset, size as usize)
    }
}

#[test]
fn abi_h160() {
    let module = TestModule::new("wasm_test/abi_classes.wasm");
    let address = H160::zero();

    // As an `Uint8Array`
    let array_buffer: AscPtr<Uint8Array> = module.asc_new(&address);
    let new_address_obj: AscPtr<Uint8Array> =
        module.takes_ptr_returns_ptr("test_address", array_buffer);

    // This should have 1 added to the first and last byte.
    let new_address: H160 = module.asc_get(new_address_obj);

    assert_eq!(
        new_address,
        H160([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1])
    )
}

#[test]
fn string() {
    let module = TestModule::new("wasm_test/abi_classes.wasm");
    let string = "    漢字Double_Me🇧🇷  ";
    let trimmed_string_obj: AscPtr<AscString> =
        module.takes_ptr_returns_ptr("repeat_twice", module.asc_new(string));
    let doubled_string: String = module.asc_get(trimmed_string_obj);
    assert_eq!(doubled_string, string.repeat(2))
}

#[test]
fn abi_big_int() {
    let module = TestModule::new("wasm_test/abi_classes.wasm");

    // Test passing in 0 and increment it by 1
    let old_uint = U256::zero();
    let array_buffer: AscPtr<AscBigInt> = module.asc_new(&BigInt::from_unsigned_u256(&old_uint));
    let new_uint_obj: AscPtr<AscBigInt> = module.takes_ptr_returns_ptr("test_uint", array_buffer);
    let new_uint: BigInt = module.asc_get(new_uint_obj);
    assert_eq!(new_uint, BigInt::from(1 as i32));
    let new_uint = new_uint.to_unsigned_u256();
    assert_eq!(new_uint, U256([1, 0, 0, 0]));

    // Test passing in -50 and increment it by 1
    let old_uint = BigInt::from(-50);
    let array_buffer: AscPtr<AscBigInt> = module.asc_new(&old_uint);
    let new_uint_obj: AscPtr<AscBigInt> = module.takes_ptr_returns_ptr("test_uint", array_buffer);
    let new_uint: BigInt = module.asc_get(new_uint_obj);
    assert_eq!(new_uint, BigInt::from(-49 as i32));
    let new_uint_from_u256 = BigInt::from_signed_u256(&new_uint.to_signed_u256());
    assert_eq!(new_uint, new_uint_from_u256);
}

#[test]
fn abi_array() {
    let module = TestModule::new("wasm_test/abi_classes.wasm");

    let vec = vec![
        "1".to_owned(),
        "2".to_owned(),
        "3".to_owned(),
        "4".to_owned(),
    ];
    let vec_obj: AscPtr<Array<AscPtr<AscString>>> = module.asc_new(&*vec);

    let new_vec_obj: AscPtr<Array<AscPtr<AscString>>> =
        module.takes_ptr_returns_ptr("test_array", vec_obj);
    let new_vec: Vec<String> = module.asc_get(new_vec_obj);

    assert_eq!(
        new_vec,
        vec![
            "1".to_owned(),
            "2".to_owned(),
            "3".to_owned(),
            "4".to_owned(),
            "5".to_owned()
        ]
    )
}

#[test]
fn abi_subarray() {
    let module = TestModule::new("wasm_test/abi_classes.wasm");

    let vec: Vec<u8> = vec![1, 2, 3, 4];
    let vec_obj: AscPtr<TypedArray<u8>> = module.asc_new(&*vec);

    let new_vec_obj: AscPtr<TypedArray<u8>> =
        module.takes_ptr_returns_ptr("byte_array_third_quarter", vec_obj);
    let new_vec: Vec<u8> = module.asc_get(new_vec_obj);

    assert_eq!(new_vec, vec![3])
}

#[test]
fn abi_bytes_and_fixed_bytes() {
    let module = TestModule::new("wasm_test/abi_classes.wasm");
    let bytes1: Vec<u8> = vec![42, 45, 7, 245, 45];
    let bytes2: Vec<u8> = vec![3, 12, 0, 1, 255];

    let new_vec_obj: AscPtr<Uint8Array> = module.takes_ptr_ptr_returns_ptr(
        "concat",
        module.asc_new::<Uint8Array, _>(&*bytes1),
        module.asc_new::<Uint8Array, _>(&*bytes2),
    );

    // This should be bytes1 and bytes2 concatenated.
    let new_vec: Vec<u8> = module.asc_get(new_vec_obj);

    let mut concated = bytes1.clone();
    concated.extend(bytes2.clone());
    assert_eq!(new_vec, concated);
}

/// Test a roundtrip Token -> Payload -> Token identity conversion through asc,
/// and assert the final token is the same as the starting one.
#[test]
fn abi_ethabi_token_identity() {
    let module = TestModule::new("wasm_test/abi_token.wasm");

    // Token::Address
    let address = H160([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    let token_address = Token::Address(address);

    let new_address_obj: AscPtr<ArrayBuffer<u8>> =
        module.takes_ptr_returns_ptr("token_to_address", module.asc_new(&token_address));

    let new_token =
        module.asc_get(module.takes_ptr_returns_ptr("token_from_address", new_address_obj));

    assert_eq!(token_address, new_token);

    // Token::Bytes
    let token_bytes = Token::Bytes(vec![42, 45, 7, 245, 45]);

    let new_bytes_obj: AscPtr<ArrayBuffer<u8>> =
        module.takes_ptr_returns_ptr("token_to_bytes", module.asc_new(&token_bytes));

    let new_token = module.asc_get(module.takes_ptr_returns_ptr("token_from_bytes", new_bytes_obj));

    assert_eq!(token_bytes, new_token);

    // Token::Int
    let int_token = Token::Int(U256([256, 453452345, 0, 42]));

    let new_int_obj: AscPtr<ArrayBuffer<u8>> =
        module.takes_ptr_returns_ptr("token_to_int", module.asc_new(&int_token));

    let new_token = module.asc_get(module.takes_ptr_returns_ptr("token_from_int", new_int_obj));

    assert_eq!(int_token, new_token);

    // Token::Uint
    let uint_token = Token::Uint(U256([256, 453452345, 0, 42]));

    let new_uint_obj: AscPtr<ArrayBuffer<u8>> =
        module.takes_ptr_returns_ptr("token_to_uint", module.asc_new(&uint_token));

    let new_token = module.asc_get(module.takes_ptr_returns_ptr("token_from_uint", new_uint_obj));

    assert_eq!(uint_token, new_token);
    assert_ne!(uint_token, int_token);

    // Token::Bool
    let token_bool = Token::Bool(true);

    let boolean: bool = module
        .module
        .invoke_export(
            "token_to_bool",
            &[RuntimeValue::from(module.asc_new(&token_bool))],
            &mut NopExternals,
        )
        .expect("call failed")
        .expect("call returned nothing")
        .try_into::<bool>()
        .expect("call did not return bool");

    let new_token = module.asc_get(
        module.takes_val_returns_ptr("token_from_bool", RuntimeValue::from(boolean as u32)),
    );

    assert_eq!(token_bool, new_token);

    // Token::String
    let token_string = Token::String("漢字Go🇧🇷".into());

    let new_string_obj: AscPtr<AscString> =
        module.takes_ptr_returns_ptr("token_to_string", module.asc_new(&token_string));

    let new_token =
        module.asc_get(module.takes_ptr_returns_ptr("token_from_string", new_string_obj));

    assert_eq!(token_string, new_token);

    // Token::Array
    let token_array = Token::Array(vec![token_address, token_bytes, token_bool]);
    let token_array_nested = Token::Array(vec![token_string, token_array]);

    let new_array_obj: AscEnumArray<EthereumValueKind> =
        module.takes_ptr_returns_ptr("token_to_array", module.asc_new(&token_array_nested));

    let new_token: Token =
        module.asc_get(module.takes_ptr_returns_ptr("token_from_array", new_array_obj));

    assert_eq!(new_token, token_array_nested);
}

#[test]
fn abi_store_value() {
    use graph::data::store::Value;

    let module = TestModule::new("wasm_test/abi_store_value.wasm");

    // Value::Null
    let null_value_ptr: AscPtr<AscEnum<StoreValueKind>> = module
        .module
        .invoke_export("value_null", &[], &mut NopExternals)
        .expect("call failed")
        .expect("call returned nothing")
        .try_into()
        .expect("call did not return ptr");
    let null_value: Value = module.asc_get(null_value_ptr);
    assert_eq!(null_value, Value::Null);

    // Value::String
    let string = "some string";
    let new_value: Value =
        module.asc_get(module.takes_ptr_returns_ptr("value_from_string", module.asc_new(string)));
    assert_eq!(new_value, Value::from(string));

    // Value::Int
    let int = i32::min_value();
    let new_value: Value =
        module.asc_get(module.takes_val_returns_ptr("value_from_int", RuntimeValue::from(int)));
    assert_eq!(new_value, Value::Int(int));

    // Value::Float
    let float: f32 = 3.14159001;
    let float_runtime = RuntimeValue::F32(F32::from_float(float));
    let new_value: Value =
        module.asc_get(module.takes_val_returns_ptr("value_from_float", float_runtime));
    assert_eq!(new_value, Value::Float(float));

    // Value::Bool
    let boolean = true;
    let new_value: Value = module.asc_get(module.takes_val_returns_ptr(
        "value_from_bool",
        RuntimeValue::I32(if boolean { 1 } else { 0 }),
    ));
    assert_eq!(new_value, Value::Bool(boolean));

    // Value::List
    let new_value: Value = module.asc_get(
        module
            .module
            .invoke_export(
                "array_from_values",
                &[RuntimeValue::from(module.asc_new(string)), float_runtime],
                &mut NopExternals,
            )
            .expect("call failed")
            .expect("call returned nothing")
            .try_into()
            .expect("call did not return ptr"),
    );
    assert_eq!(
        new_value,
        Value::List(vec![Value::from(string), Value::Float(float)])
    );

    let array: &[Value] = &[
        Value::String("foo".to_owned()),
        Value::String("bar".to_owned()),
    ];
    let array_ptr = module.asc_new(array);
    let new_value: Value =
        module.asc_get(module.takes_ptr_returns_ptr("value_from_array", array_ptr));
    assert_eq!(
        new_value,
        Value::List(vec![
            Value::String("foo".to_owned()),
            Value::String("bar".to_owned()),
        ])
    );

    // Value::Bytes
    let bytes: &[u8] = &[0, 2, 5];
    let bytes_ptr: AscPtr<Bytes> = module.asc_new(bytes);
    let new_value: Value =
        module.asc_get(module.takes_ptr_returns_ptr("value_from_bytes", bytes_ptr));
    assert_eq!(new_value, Value::Bytes(bytes.into()));

    // Value::BigInt
    let bytes: &[u8] = &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1];
    let bytes_ptr: AscPtr<Uint8Array> = module.asc_new(bytes);
    let new_value: Value =
        module.asc_get(module.takes_ptr_returns_ptr("value_from_bigint", bytes_ptr));
    assert_eq!(
        new_value,
        Value::BigInt(::graph::data::store::scalar::BigInt::from_unsigned_bytes_le(bytes))
    );
}
