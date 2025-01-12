use crate::{keyer, math, Erc20Token, Pool};
use std::ops::{Div, Mul};
use std::str;
use std::str::FromStr;
use substreams::log;
use substreams::scalar::{BigDecimal, BigInt};
use substreams::store::{StoreGet, StoreGetBigDecimal, StoreGetBigInt, StoreGetProto, StoreGetRaw};

const USDC_WETH_03_POOL: &str = "8ad599c3a0ff1de082011efddc58f1908eb6e6d8";
const USDC_ADDRESS: &str = "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48";
const WETH_ADDRESS: &str = "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2";

pub const STABLE_COINS: [&str; 6] = [
    "6b175474e89094c44da98b954eedeac495271d0f",
    "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
    "dac17f958d2ee523a2206206994597c13d831ec7",
    "0000000000085d4780b73119b644ae5ecd22b376",
    "956f47f50a910163d8bf957cf5846d573e7f87ca",
    "4dd28568d05f09b02220b09c2cb307bfd837cb95",
];

pub const WHITELIST_TOKENS: [&str; 21] = [
    "c02aaa39b223fe8d0a0e5c4f27ead9083c756cc2", // WETH
    "6b175474e89094c44da98b954eedeac495271d0f", // DAI
    "a0b86991c6218b36c1d19d4a2e9eb0ce3606eb48", // USDC
    "dac17f958d2ee523a2206206994597c13d831ec7", // USDT
    "0000000000085d4780b73119b644ae5ecd22b376", // TUSD
    "2260fac5e5542a773aa44fbcfedf7c193bc2c599", // WBTC
    "5d3a536e4d6dbd6114cc1ead35777bab948e3643", // cDAI
    "39aa39c021dfbae8fac545936693ac917d5e7563", // cUSDC
    "86fadb80d8d2cff3c3680819e4da99c10232ba0f", // EBASE
    "57ab1ec28d129707052df4df418d58a2d46d5f51", // sUSD
    "9f8f72aa9304c8b593d555f12ef6589cc3a579a2", // MKR
    "c00e94cb662c3520282e6f5717214004a7f26888", // COMP
    "514910771af9ca656af840dff83e8264ecf986ca", // LINK
    "c011a73ee8576fb46f5e1c5751ca3b9fe0af2a6f", // SNX
    "0bc529c00c6401aef6d220be8c6ea1667f6ad93e", // YFI
    "111111111117dc0aa78b770fa6a738034120c302", // 1INCH
    "df5e0e81dff6faf3a7e52ba697820c5e32d806a8", // yCurv
    "956f47f50a910163d8bf957cf5846d573e7f87ca", // FEI
    "7d1afa7b718fb893db30a3abc0cfc608aacfebb0", // MATIC
    "7fc66500c84a76ad7e9c93437bfc5ac33e2ddae9", // AAVE
    "fe2e637202056d30016725477c5da089ab0a043a", // sETH2
];

pub fn sqrt_price_x96_to_token_prices(
    sqrt_price: BigDecimal,
    token_0: &Erc20Token,
    token_1: &Erc20Token,
) -> (BigDecimal, BigDecimal) {
    log::debug!(
        "Computing prices for {} {} and {} {}",
        token_0.symbol,
        token_0.decimals,
        token_1.symbol,
        token_1.decimals
    );

    let price: BigDecimal = sqrt_price.clone().mul(sqrt_price);
    let token0_decimals: BigInt = BigInt::from(token_0.decimals);
    let token1_decimals: BigInt = BigInt::from(token_1.decimals);
    let denominator: BigDecimal =
        BigDecimal::from_str("6277101735386680763835789423207666416102355444464034512896").unwrap();

    let price1 = price
        .div(denominator)
        .mul(math::exponent_to_big_decimal(&token0_decimals))
        .div(math::exponent_to_big_decimal(&token1_decimals));

    log::info!("price1: {}", price1);
    let price0 = math::safe_div(&BigDecimal::one(), &price1);

    return (price0, price1);
}

pub fn find_eth_per_token(
    log_ordinal: u64,
    pool_address: &String,
    token_address: &String,
    pools_store: &StoreGetProto<Pool>,
    pool_liquidities_store: &StoreGetBigInt,
    tokens_whitelist_pools_store: &StoreGetRaw,
    total_native_value_locked_store: &StoreGetBigDecimal,
    prices_store: &StoreGetBigDecimal,
) -> BigDecimal {
    log::debug!(
        "finding ETH per token for {} in pool {}",
        token_address,
        pool_address
    );
    if token_address.eq(WETH_ADDRESS) {
        log::debug!("is ETH return 1");
        return BigDecimal::one();
    }

    let mut price_so_far = BigDecimal::zero().with_prec(100);

    if STABLE_COINS.contains(&token_address.as_str()) {
        log::debug!("token addr: {} is a stable coin", token_address);
        let eth_price_usd = get_eth_price_in_usd(prices_store, log_ordinal);
        price_so_far = math::safe_div(&BigDecimal::one(), &eth_price_usd);
    } else {
        // TODO: @eduard change this once the changes for store of list has been merged
        let wl = match tokens_whitelist_pools_store
            .get_last(&keyer::token_pool_whitelist(token_address))
        {
            None => {
                log::debug!(
                    "failed to get whitelisted pools for token {}",
                    token_address
                );
                return BigDecimal::zero();
            }
            Some(bytes) => String::from_utf8(bytes.to_vec()).unwrap(),
        };

        let mut whitelisted_pools: Vec<&str> = vec![];
        for p in wl.split(";") {
            if !p.is_empty() {
                whitelisted_pools.push(p);
            }
        }
        log::debug!("found whitelisted pools {}", whitelisted_pools.len());

        let mut largest_eth_locked = BigDecimal::zero().with_prec(100);
        let minimum_eth_locked = BigDecimal::from_str("60").unwrap();
        let mut eth_locked: BigDecimal;

        for pool_address in whitelisted_pools.iter() {
            log::debug!("checking pool: {}", pool_address);
            let pool = match pools_store.get_last(keyer::pool_key(&pool_address.to_string())) {
                None => continue,
                Some(p) => p,
            };
            let token0 = pool.token0.as_ref().unwrap();
            let token1 = pool.token1.as_ref().unwrap();
            log::debug!(
                "found pool: {} with token0 {} with token1 {}",
                pool.address,
                token0.address,
                token1.address
            );

            let liquidity: BigInt =
                match pool_liquidities_store.get_last(&keyer::pool_liquidity(&pool.address)) {
                    None => {
                        log::debug!("No liquidity for pool {}", pool.address);
                        BigInt::zero()
                    }
                    Some(price) => price,
                };

            if liquidity.gt(&BigInt::zero()) {
                if &token0.address == token_address {
                    log::info!(
                        "current pool token 0 matches desired token, complementary token is {} {}",
                        token1.address,
                        token1.symbol
                    );
                    let native_locked_value: BigDecimal = match total_native_value_locked_store
                        .get_last(&keyer::pool_native_total_value_locked_token(
                            &pool.address,
                            &token1.address,
                        )) {
                        None => BigDecimal::zero().with_prec(100),
                        Some(price) => price.with_prec(100),
                    };
                    log::debug!(
                        "native locked value of token1 in pool {}",
                        native_locked_value
                    );

                    // If the counter token is WETH we know the derived price is 1
                    if token1.address.eq(WETH_ADDRESS) {
                        log::debug!("token 1 is WETH");
                        eth_locked = native_locked_value
                    } else {
                        log::debug!("token 1 is NOT WETH");
                        let token_eth_price: BigDecimal = match prices_store.get_at(
                            log_ordinal,
                            &keyer::prices_token_pair(&token1.address, &WETH_ADDRESS.to_string()),
                        ) {
                            None => {
                                log::debug!(
                                    "unable to find token 1 price in eth {:?}",
                                    token1.address
                                );
                                continue;
                            }
                            Some(price) => price,
                        };
                        log::debug!("token 1 is price in eth {}", token_eth_price);
                        eth_locked = native_locked_value.mul(token_eth_price);
                        log::debug!("computed eth locked {}", eth_locked);
                    }
                    log::debug!(
                        "eth locked in pool {} {} (largest {})",
                        pool.address,
                        eth_locked,
                        largest_eth_locked
                    );
                    if eth_locked.gt(&largest_eth_locked) && eth_locked.gt(&minimum_eth_locked) {
                        log::debug!("eth locked passed test");
                        let token1_price: BigDecimal = match prices_store.get_at(
                            log_ordinal,
                            &keyer::prices_pool_token_key(
                                &pool.address,
                                &token1.address,
                                "token1".to_string(),
                            ),
                        ) {
                            None => {
                                log::debug!(
                                    "unable to find pool {} for token {} price",
                                    pool.address,
                                    token1.address
                                );
                                continue;
                            }
                            Some(price) => price,
                        };
                        log::debug!("found token 1 price {}", token1_price);
                        largest_eth_locked = eth_locked.clone();
                        price_so_far = token1_price.mul(eth_locked.clone());
                        log::debug!("price_so_far {}", price_so_far);
                    }
                }
                if &token1.address == token_address {
                    log::debug!(
                        "current pool token 1 matches desired token, complementary token is {} {}",
                        token0.address,
                        token1.symbol
                    );
                    let native_locked_value: BigDecimal = match total_native_value_locked_store
                        .get_last(&keyer::pool_native_total_value_locked_token(
                            &pool.address,
                            &token0.address,
                        )) {
                        None => BigDecimal::zero().with_prec(100),
                        Some(price) => price.with_prec(100),
                    };
                    log::debug!(
                        "native locked value of token0 in pool {}",
                        native_locked_value
                    );

                    // If the counter token is WETH we know the derived price is 1
                    if token0.address.eq(WETH_ADDRESS) {
                        log::debug!("token 0 is WETH");
                        eth_locked = native_locked_value
                    } else {
                        log::debug!("token 0 is NOT WETH");
                        let token_eth_price: BigDecimal = match prices_store.get_at(
                            log_ordinal,
                            &keyer::prices_token_pair(&token0.address, &WETH_ADDRESS.to_string()),
                        ) {
                            None => {
                                log::debug!(
                                    "unable to find token 0 price in eth {:?}",
                                    token0.address
                                );
                                continue;
                            }
                            Some(price) => price,
                        };
                        log::debug!("token 0 is price in eth {}", token_eth_price);
                        eth_locked = native_locked_value.mul(token_eth_price);
                        log::debug!("computed eth locked {}", eth_locked);
                    }
                    log::debug!(
                        "eth locked in pool {} {} (largest {})",
                        pool.address,
                        eth_locked,
                        largest_eth_locked
                    );
                    if eth_locked.gt(&largest_eth_locked) && eth_locked.gt(&minimum_eth_locked) {
                        log::debug!("eth locked passed test");
                        let token0_price: BigDecimal = match prices_store.get_at(
                            log_ordinal,
                            &keyer::prices_pool_token_key(
                                &pool.address,
                                &token0.address,
                                "token0".to_string(),
                            ),
                        ) {
                            None => {
                                log::debug!(
                                    "unable to find pool {} for token {} price",
                                    pool.address,
                                    token0.address
                                );
                                continue;
                            }
                            Some(price) => price,
                        };
                        log::debug!("found token 0 price {}", token0_price);
                        largest_eth_locked = eth_locked.clone();
                        price_so_far = token0_price.mul(eth_locked.clone());
                        log::debug!("price_so_far {}", price_so_far);
                    }
                }
            }
        }
    }
    return price_so_far;
}

pub fn get_eth_price_in_usd(prices_store: &StoreGetBigDecimal, ordinal: u64) -> BigDecimal {
    // USDC is the token0 in this pool kinda same point as
    // mentioned earlier, token0 hard-coded is not clean
    let key = keyer::prices_pool_token_key(
        &USDC_WETH_03_POOL.to_string(),
        &USDC_ADDRESS.to_string(),
        "token0".to_string(),
    );
    return match prices_store.get_at(ordinal, &key) {
        None => {
            log::debug!("price not found");
            BigDecimal::zero()
        }
        Some(price) => price,
    };
}
