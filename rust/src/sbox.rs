extern crate dirs;
extern crate rand;
extern crate json;
extern crate users;

mod base85;
mod spritz;

use rand::Rng;
use base85::{encode85, decode85};
use spritz::{test_aead, test_hash, test_output, hash, aead, aead_decrypt};
use std::collections::HashMap;

fn main() {
  base85::run_tests();

  test_output("ABC",     &decode85("MLLTuyBE.B"));
  test_output("spam",    &decode85("v<pd0vU-[@"));
  test_output("arcfour", &decode85(".b%F~r%Dh;"));

  test_hash("ABC",     &decode85("!n{-gSr&iS"));
  test_hash("spam",    &decode85("`Rs3^;A9U3"));
  test_hash("arcfour", &decode85("{2ESf%~&2j"));

  test_aead("ABC",  &decode85("Rv0(1hs@aK7O^;R-I4^ss^SC6Q-pB*x!M4&kJm:PyQXV"));
  test_aead(
    "spam",
    &decode85("e%{bhFYJ__;BBc>d~{_eHnBAEU7*-{I<+wZYxIO7@j4d_"),
  );
  test_aead(
    "arcfour",
    &decode85("_<L|Qat+pGNrAs+Xc!R|vi8v%4axmYPr~ac&We.wJ;9iiPjZ+"),
  );

  test_keyid("my_key_id", "mVU!c-nS?_");
  test_keyid("ABC",       "isZ{2O{{&8");
  test_keyid("spam",      "ZT+[pIT.OQ");
  test_keyid("arcfour",   "V.|2:mM`g(");

  test_sbox(
    "%Cl*awJGQB/!!!!!!!!!!!!!!!/NWuTFJnH>99c5b_L0-k7FzNB|2-6/`j3|\
     7XFwj^sll#C.G4>v%EJo!AQz;Vb5mmcIMkgBK&cLB@C>m=.w074[lVu#r?~",
    ("{\"scope\":\"test_scope\"}".to_string(),
     Ok("this is some data!".to_string())),
  );
  test_sbox(
    "%Cl*awJGQB/!!!!!!!!!!!!!!!/NWuTFJnH>99c5b_L0-k7FzNB|2-6/agq~\
     IqSSb1h4a.H0_@<{&kjL!rR(ORtq4+uf~*%.qnofsHf7q",
    ("{\"scope\":\"test_scope\"}".to_string(),
     Ok("woo hoo".to_string())),
  );

  println!("Pass");
}

fn test_sbox(
  expected_boxed: &str,
  expected_unboxed: (
    std::string::String,
    std::result::Result<std::string::String, &str>,
  ),
){
  // file: test_scope.keyring
  // "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

  let actual_unboxed = unsbox(expected_boxed, "test_scope");
  assert_eq!(actual_unboxed, expected_unboxed);

  let actual_boxed = sbox_with_headers_scope_and_nonce(
    expected_unboxed.1.unwrap().as_bytes(),
    HashMap::new(),
    "test_scope",
    &[0u8; 12],
  );
  assert_eq!(actual_boxed, expected_boxed);
}

fn keyid(key: &[u8]) -> String { encode85(&hash(&key, 8)) }

fn test_keyid(key: &str, expected_keyid: &str){
  let actual_keyid = keyid(&key.as_bytes());
  // println!("{:?} {:?}", actual_keyid, expected_keyid);
  assert_eq!(actual_keyid, expected_keyid);
}

pub fn sbox(data: &[u8]) -> String {sbox_with_headers(data, HashMap::new())}

pub fn sbox_with_headers(
  data: &[u8],
  headers: HashMap<String, String>
) -> String {
  sbox_with_headers_and_scope(
    data,
    headers,
    &users::get_current_username().unwrap().into_string().unwrap(),
  )
}

pub fn sbox_with_scope(data: &[u8], scope: &str) -> String {
  sbox_with_headers_and_scope(data, HashMap::new(), scope)
}

pub fn sbox_with_headers_and_scope(
  data: &[u8],
  headers: HashMap<String, String>,
  scope: &str,
) -> String {
  sbox_with_headers_scope_and_nonce(data, headers, scope, &gen_nonce())
}

fn sbox_with_headers_scope_and_nonce(
  data: &[u8],
  mut headers: HashMap<String, String>,
  scope: &str,
  nonce: &[u8]
) -> String {
  let mut keyring: HashMap<String, HashMap<String, Vec<u8>>> = HashMap::new();
  let keys_str = read_scope(scope);
  let current_key = add_scope(&mut keyring, &keys_str, scope);

  headers.insert("scope".to_string(), scope.to_string());
  let header = json::stringify(headers);
  let ciphertext = aead(&current_key, &nonce, &header.as_bytes(), data, 32);

  // keyid/nonce/header/ciphertext all in base85
  vec![
    keyid(&current_key),
    encode85(&nonce),
    encode85(header.as_bytes()),
    encode85(&ciphertext),
  ].join("/")
}

fn unsbox(msg: &str, scope: &str) -> (String, Result<String, &'static str>){
  let mut keyring: HashMap<String, HashMap<String, Vec<u8>>> = HashMap::new();
  let keys_str = read_scope(scope);
  add_scope(&mut keyring, &keys_str, scope);

  let mut parts = msg.split('/');
  let key = keyring
    .get(scope).expect("scope not in keyring")
    .get(parts.next().unwrap()).expect("key not in scope");
  let nonce = decode85(&parts.next().unwrap());
  let header = decode85(&parts.next().unwrap());
  let ciphertext = decode85(parts.next().unwrap());
  let msg_data = aead_decrypt(&key, &nonce, &header, &ciphertext, 32);
  return (String::from_utf8(header).unwrap(), msg_data)
}

fn read_scope(scope_name: &str) -> String {
  let mut filename = dirs::home_dir().expect("home_dir not found");
  filename.push(".sbox");
  filename.push(scope_name);
  filename.set_extension("keyring");
  return std::fs::read_to_string(filename).expect("keyring file");
}

fn add_scope(
  // keyring = {scope: {key_id: key}}
  keyring: &mut HashMap<String, HashMap<String, Vec<u8>>>,
  keys_str: &str,
  scope: &str,
) -> Vec<u8> {
  keyring.insert(scope.to_string(), HashMap::new());

  let scope_keys = keyring.get_mut(scope).unwrap();
  let mut lastkey = Vec::new();
  for row in keys_str.split('\n'){
    if !row.is_empty() {
      let key85 = row.split(' ').next().unwrap();
      let key = decode85(key85);
      lastkey = key.clone();
      scope_keys.insert(keyid(&key), key);
    }
  }
  return lastkey
}

fn gen_nonce() -> [u8; 12] {
  let mut rng = rand::thread_rng();
  [
    rng.gen(), rng.gen(), rng.gen(), rng.gen(), rng.gen(), rng.gen(),
    rng.gen(), rng.gen(), rng.gen(), rng.gen(), rng.gen(), rng.gen(),
  ]
}