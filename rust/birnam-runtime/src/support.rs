//! Test/diagnostic factories; production receives its completed inputs explicitly.
pub(crate) fn diagnostic() -> birnam_abi::runtime_support::FactoriesV1 {
    birnam_abi::runtime_support::FactoriesV1 {
        version: 1, byte_size: size_of::<birnam_abi::runtime_support::FactoriesV1>(),
        #[cfg(test)]
        observation: unsafe { *birnam_method_observation::birnam_method_observation_factory_v1() },
        #[cfg(not(test))]
        observation: unsafe { birnam_abi::method_observation::operations() },
        #[cfg(test)]
        storage: unsafe { *birnam_storage::birnam_storage_observation_factory_v1() },
        #[cfg(not(test))]
        storage: unsafe { birnam_abi::storage_observation::operations() },
        #[cfg(test)]
        identity: unsafe { *birnam_storage::birnam_deferred_identity_factory_v1() },
        #[cfg(not(test))]
        identity: unsafe { birnam_abi::deferred_identity::operations() },
        #[cfg(test)]
        index: unsafe { *birnam_storage::birnam_deferred_index_factory_v1() },
        #[cfg(not(test))]
        index: unsafe { birnam_abi::deferred_index::operations() },
        #[cfg(test)]
        cache: unsafe { *birnam_dispatch_cache::birnam_dispatch_cache_factory_v2() },
        #[cfg(not(test))]
        cache: unsafe { birnam_abi::dispatch_cache::operations() },
        #[cfg(test)]
        verdict: unsafe { *birnam_dispatch_cache::birnam_guard_verdict_factory_v1() },
        #[cfg(not(test))]
        verdict: unsafe { birnam_abi::guard_verdict::operations() },
    }
}
