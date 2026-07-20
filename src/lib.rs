#![no_std]
use soroban_sdk::{
    contract, contractimpl, contracttype, Address, Env, String, token,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Tld {
    Kale,
    Farm,
    Fun,
    Kalien,
    Farmer,
    Custom(String),
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FullDomain {
    pub name: String,
    pub tld: Tld,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DataKey {
    Admin,
    KaleToken,
    FlatFee,
    Domain(FullDomain),
    Listing(FullDomain),
}

#[contract]
pub struct KaleDomainRegistry;

#[contractimpl]
impl KaleDomainRegistry {
    /// Inisialisasi kontrak
    pub fn initialize(env: Env, admin: Address, kale_token: Address, flat_fee: i128) {
        if env.storage().instance().has(&DataKey::Admin) {
            panic!("already initialized");
        }
        let fee_in_stroops = flat_fee * 10_000_000;
        
        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::KaleToken, &kale_token);
        env.storage().instance().set(&DataKey::FlatFee, &fee_in_stroops);
    }

    /// Mendaftarkan domain baru (Minting)
    pub fn register(env: Env, name: String, tld: Tld, owner: Address) {
        owner.require_auth();

        let len = name.len();
        if len < 3 || len > 15 {
            panic!("Panjang nama harus antara 3 hingga 15 karakter");
        }

        let mut fee_to_charge = env.storage().instance().get::<_, i128>(&DataKey::FlatFee).unwrap();

        if let Tld::Custom(ext) = &tld {
            let ext_len = ext.len();
            if ext_len < 3 || ext_len > 5 {
                panic!("Panjang custom extention harus antara 3 hingga 5 karakter");
            }

            // Convert to byte array to check characters
            let mut is_official = false;
            
            // Check collision with official TLDs
            if *ext == String::from_str(&env, "kale")
                || *ext == String::from_str(&env, "farm")
                || *ext == String::from_str(&env, "fun")
                || *ext == String::from_str(&env, "kalien")
                || *ext == String::from_str(&env, "farmer")
            {
                is_official = true;
            }
            if is_official {
                panic!("Nama ekstensi ini adalah ekstensi resmi dan tidak dapat dikustomisasi");
            }
        }

        let full_domain = FullDomain { name: name.clone(), tld: tld.clone() };
        let domain_key = DataKey::Domain(full_domain);
        if env.storage().persistent().has(&domain_key) {
            panic!("Domain sudah terdaftar!");
        }

        let admin: Address = env.storage().instance().get(&DataKey::Admin).unwrap();
        let kale_token: Address = env.storage().instance().get(&DataKey::KaleToken).unwrap();

        // Add premium fee for Custom
        if let Tld::Custom(_) = &tld {
            fee_to_charge += 500 * 10_000_000;
        }

        let token_client = token::Client::new(&env, &kale_token);
        token_client.transfer(&owner, &admin, &fee_to_charge);

        env.storage().persistent().set(&domain_key, &owner);
    }

    /// Mentransfer domain (P2P gratis)
    pub fn transfer_domain(env: Env, name: String, tld: Tld, from: Address, to: Address) {
        from.require_auth();

        let full_domain = FullDomain { name: name.clone(), tld: tld.clone() };
        let domain_key = DataKey::Domain(full_domain.clone());
        let listing_key = DataKey::Listing(full_domain);

        // Security check: cannot transfer if listed
        if env.storage().persistent().has(&listing_key) {
            panic!("Domain sedang dalam status dijual (Listed). Batalkan penjualan (Unlist) terlebih dahulu.");
        }
        
        if let Some(current_owner) = env.storage().persistent().get::<DataKey, Address>(&domain_key) {
            if current_owner != from {
                panic!("Anda bukan pemilik domain ini");
            }
            env.storage().persistent().set(&domain_key, &to);
        } else {
            panic!("Domain tidak ditemukan");
        }
    }

    /// Memasang harga jual untuk domain (Listing)
    pub fn list_domain(env: Env, name: String, tld: Tld, owner: Address, price: i128) {
        owner.require_auth();

        let full_domain = FullDomain { name, tld };
        let domain_key = DataKey::Domain(full_domain.clone());

        if let Some(current_owner) = env.storage().persistent().get::<DataKey, Address>(&domain_key) {
            if current_owner != owner {
                panic!("Anda bukan pemilik domain ini");
            }
            
            // Konversi ke stroops
            let price_in_stroops = price * 10_000_000;
            let listing_key = DataKey::Listing(full_domain);
            env.storage().persistent().set(&listing_key, &price_in_stroops);
        } else {
            panic!("Domain tidak ditemukan");
        }
    }

    /// Membatalkan penjualan domain (Unlisting)
    pub fn unlist_domain(env: Env, name: String, tld: Tld, owner: Address) {
        owner.require_auth();

        let full_domain = FullDomain { name, tld };
        let domain_key = DataKey::Domain(full_domain.clone());

        if let Some(current_owner) = env.storage().persistent().get::<DataKey, Address>(&domain_key) {
            if current_owner != owner {
                panic!("Anda bukan pemilik domain ini");
            }
            
            let listing_key = DataKey::Listing(full_domain);
            if env.storage().persistent().has(&listing_key) {
                env.storage().persistent().remove(&listing_key);
            } else {
                panic!("Domain ini tidak sedang dijual");
            }
        } else {
            panic!("Domain tidak ditemukan");
        }
    }

    /// Membeli domain yang sedang dijual (Redeem/Atomic Swap)
    pub fn redeem_domain(env: Env, name: String, tld: Tld, buyer: Address) {
        buyer.require_auth();

        let full_domain = FullDomain { name: name.clone(), tld: tld.clone() };
        let domain_key = DataKey::Domain(full_domain.clone());
        let listing_key = DataKey::Listing(full_domain);

        let seller: Address = env.storage().persistent().get(&domain_key).expect("Domain tidak ditemukan");
        let price_in_stroops: i128 = env.storage().persistent().get(&listing_key).expect("Domain tidak sedang dijual");

        let kale_token: Address = env.storage().instance().get(&DataKey::KaleToken).unwrap();
        let token_client = token::Client::new(&env, &kale_token);

        // Transfer KALE 100% ke seller
        token_client.transfer(&buyer, &seller, &price_in_stroops);

        // Transfer Domain NFT ke buyer
        env.storage().persistent().set(&domain_key, &buyer);

        // Hapus status penjualan
        env.storage().persistent().remove(&listing_key);
    }

    /// Membaca/Menerjemahkan domain menjadi alamat dompet (Resolve)
    pub fn resolve(env: Env, name: String, tld: Tld) -> Address {
        let full_domain = FullDomain { name, tld };
        let domain_key = DataKey::Domain(full_domain);
        env.storage().persistent().get(&domain_key).expect("Domain tidak terdaftar")
    }
}
