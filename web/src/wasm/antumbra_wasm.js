/* @ts-self-types="./antumbra_wasm.d.ts" */

export class Prover {
    __destroy_into_raw() {
        const ptr = this.__wbg_ptr;
        this.__wbg_ptr = 0;
        ProverFinalization.unregister(this);
        return ptr;
    }
    free() {
        const ptr = this.__destroy_into_raw();
        wasm.__wbg_prover_free(ptr, 0);
    }
    /**
     * @param {Uint8Array | number[]} joinsplit
     * @param {Uint8Array | number[]} swap
     * @param {Uint8Array | number[]} swapclaim
     */
    constructor(joinsplit, swap, swapclaim) {
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            let ptr0, len0, ptr1, len1, ptr2, len2;
            try {
                ptr0 = passArray8ToWasm0(joinsplit, wasm.__wbindgen_export2);
                len0 = WASM_VECTOR_LEN;
                ptr1 = passArray8ToWasm0(swap, wasm.__wbindgen_export2);
                len1 = WASM_VECTOR_LEN;
                ptr2 = passArray8ToWasm0(swapclaim, wasm.__wbindgen_export2);
                len2 = WASM_VECTOR_LEN;
            } catch (e) {
                if (ptr2) { wasm.__wbindgen_export(ptr2, len2 * 1, 1); }
                if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
                if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
                throw e;
            }
            wasm.prover_new(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            if (r2) {
                throw takeFromExternrefTable0(r1);
            }
            this.__wbg_ptr = r0;
            ProverFinalization.register(this, this.__wbg_ptr, this);
            return this;
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
        }
    }
    /**
     * Prepared `swap_claim` call for swap `index`. `batch_json` is the contract's
     * `get_batch` view. Returns `{ prepared, outputs: [{asset_id, amount}, ...] }`.
     * @param {string} wallet_json
     * @param {number} index
     * @param {number} asset_1
     * @param {number} asset_2
     * @param {string} batch_json
     * @param {Uint8Array | number[]} seed_bytes
     * @returns {string}
     */
    prepare_claim(wallet_json, index, asset_1, asset_2, batch_json, seed_bytes) {
        let deferred8_0;
        let deferred8_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const val4 = +index;
            const val5 = +asset_1;
            const val6 = +asset_2;
            let ptr0, len0, ptr1, len1, ptr2, len2;
            try {
                ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len0 = WASM_VECTOR_LEN;
                ptr1 = passStringToWasm0(batch_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len1 = WASM_VECTOR_LEN;
                ptr2 = passArray8ToWasm0(seed_bytes, wasm.__wbindgen_export2);
                len2 = WASM_VECTOR_LEN;
            } catch (e) {
                if (ptr2) { wasm.__wbindgen_export(ptr2, len2 * 1, 1); }
                if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
                if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
                throw e;
            }
            wasm.prover_prepare_claim(retptr, this.__wbg_ptr, ptr0, len0, val4, val5, val6, ptr1, len1, ptr2, len2);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr7 = r0;
            var len7 = r1;
            if (r3) {
                ptr7 = 0; len7 = 0;
                throw takeFromExternrefTable0(r2);
            }
            deferred8_0 = ptr7;
            deferred8_1 = len7;
            return getStringFromWasm0(ptr7, len7);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred8_0, deferred8_1, 1);
        }
    }
    /**
     * Prepared `swap` call. `epoch` is the contract's current epoch.
     * @param {string} wallet_json
     * @param {number} pair_id
     * @param {number} asset_1
     * @param {number} asset_2
     * @param {number} sell_asset
     * @param {string} amount_str
     * @param {number} epoch
     * @param {string} pk_x
     * @param {string} pk_y
     * @param {Uint8Array | number[]} seed_bytes
     * @returns {string}
     */
    prepare_swap(wallet_json, pair_id, asset_1, asset_2, sell_asset, amount_str, epoch, pk_x, pk_y, seed_bytes) {
        let deferred12_0;
        let deferred12_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const val6 = +pair_id;
            const val7 = +asset_1;
            const val8 = +asset_2;
            const val9 = +sell_asset;
            const val10 = +epoch;
            let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3, ptr4, len4;
            try {
                ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len0 = WASM_VECTOR_LEN;
                ptr1 = passStringToWasm0(amount_str, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len1 = WASM_VECTOR_LEN;
                ptr2 = passStringToWasm0(pk_x, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len2 = WASM_VECTOR_LEN;
                ptr3 = passStringToWasm0(pk_y, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len3 = WASM_VECTOR_LEN;
                ptr4 = passArray8ToWasm0(seed_bytes, wasm.__wbindgen_export2);
                len4 = WASM_VECTOR_LEN;
            } catch (e) {
                if (ptr4) { wasm.__wbindgen_export(ptr4, len4 * 1, 1); }
                if (ptr3) { wasm.__wbindgen_export(ptr3, len3 * 1, 1); }
                if (ptr2) { wasm.__wbindgen_export(ptr2, len2 * 1, 1); }
                if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
                if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
                throw e;
            }
            wasm.prover_prepare_swap(retptr, this.__wbg_ptr, ptr0, len0, val6, val7, val8, val9, ptr1, len1, val10, ptr2, len2, ptr3, len3, ptr4, len4);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr11 = r0;
            var len11 = r1;
            if (r3) {
                ptr11 = 0; len11 = 0;
                throw takeFromExternrefTable0(r2);
            }
            deferred12_0 = ptr11;
            deferred12_1 = len11;
            return getStringFromWasm0(ptr11, len11);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred12_0, deferred12_1, 1);
        }
    }
    /**
     * Prepared `joinsplit` call for a private transfer.
     * @param {string} wallet_json
     * @param {number} asset_id
     * @param {string} amount_str
     * @param {string} to_address
     * @param {Uint8Array | number[]} seed_bytes
     * @returns {string}
     */
    prepare_transfer(wallet_json, asset_id, amount_str, to_address, seed_bytes) {
        let deferred7_0;
        let deferred7_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const val5 = +asset_id;
            let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3;
            try {
                ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len0 = WASM_VECTOR_LEN;
                ptr1 = passStringToWasm0(amount_str, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len1 = WASM_VECTOR_LEN;
                ptr2 = passStringToWasm0(to_address, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len2 = WASM_VECTOR_LEN;
                ptr3 = passArray8ToWasm0(seed_bytes, wasm.__wbindgen_export2);
                len3 = WASM_VECTOR_LEN;
            } catch (e) {
                if (ptr3) { wasm.__wbindgen_export(ptr3, len3 * 1, 1); }
                if (ptr2) { wasm.__wbindgen_export(ptr2, len2 * 1, 1); }
                if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
                if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
                throw e;
            }
            wasm.prover_prepare_transfer(retptr, this.__wbg_ptr, ptr0, len0, val5, ptr1, len1, ptr2, len2, ptr3, len3);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr6 = r0;
            var len6 = r1;
            if (r3) {
                ptr6 = 0; len6 = 0;
                throw takeFromExternrefTable0(r2);
            }
            deferred7_0 = ptr6;
            deferred7_1 = len6;
            return getStringFromWasm0(ptr6, len6);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred7_0, deferred7_1, 1);
        }
    }
    /**
     * Prepared `joinsplit` call for a withdrawal to a NEAR account.
     * @param {string} wallet_json
     * @param {number} asset_id
     * @param {string} amount_str
     * @param {string} to_account
     * @param {Uint8Array | number[]} seed_bytes
     * @returns {string}
     */
    prepare_withdraw(wallet_json, asset_id, amount_str, to_account, seed_bytes) {
        let deferred7_0;
        let deferred7_1;
        try {
            const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
            const val5 = +asset_id;
            let ptr0, len0, ptr1, len1, ptr2, len2, ptr3, len3;
            try {
                ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len0 = WASM_VECTOR_LEN;
                ptr1 = passStringToWasm0(amount_str, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len1 = WASM_VECTOR_LEN;
                ptr2 = passStringToWasm0(to_account, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
                len2 = WASM_VECTOR_LEN;
                ptr3 = passArray8ToWasm0(seed_bytes, wasm.__wbindgen_export2);
                len3 = WASM_VECTOR_LEN;
            } catch (e) {
                if (ptr3) { wasm.__wbindgen_export(ptr3, len3 * 1, 1); }
                if (ptr2) { wasm.__wbindgen_export(ptr2, len2 * 1, 1); }
                if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
                if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
                throw e;
            }
            wasm.prover_prepare_withdraw(retptr, this.__wbg_ptr, ptr0, len0, val5, ptr1, len1, ptr2, len2, ptr3, len3);
            var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
            var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
            var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
            var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
            var ptr6 = r0;
            var len6 = r1;
            if (r3) {
                ptr6 = 0; len6 = 0;
                throw takeFromExternrefTable0(r2);
            }
            deferred7_0 = ptr6;
            deferred7_1 = len6;
            return getStringFromWasm0(ptr6, len6);
        } finally {
            wasm.__wbindgen_add_to_stack_pointer(16);
            wasm.__wbindgen_export(deferred7_0, deferred7_1, 1);
        }
    }
}
if (Symbol.dispose) Prover.prototype[Symbol.dispose] = Prover.prototype.free;

/**
 * Applies a prepared call's bookkeeping once its transaction succeeded. Returns the wallet.
 * @param {string} wallet_json
 * @param {string} prepared_json
 * @returns {string}
 */
export function apply_prepared(wallet_json, prepared_json) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        let ptr0, len0, ptr1, len1;
        try {
            ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(prepared_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len1 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.apply_prepared(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr3 = r0;
        var len3 = r1;
        if (r3) {
            ptr3 = 0; len3 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred4_0 = ptr3;
        deferred4_1 = len3;
        return getStringFromWasm0(ptr3, len3);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred4_0, deferred4_1, 1);
    }
}

/**
 * Marks notes/swaps (by index) spent/claimed after the chain confirmed it. Returns the wallet.
 * @param {string} wallet_json
 * @param {string} note_idxs_json
 * @param {string} swap_idxs_json
 * @returns {string}
 */
export function apply_spent(wallet_json, note_idxs_json, swap_idxs_json) {
    let deferred5_0;
    let deferred5_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        let ptr0, len0, ptr1, len1, ptr2, len2;
        try {
            ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(note_idxs_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len1 = WASM_VECTOR_LEN;
            ptr2 = passStringToWasm0(swap_idxs_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len2 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr2) { wasm.__wbindgen_export(ptr2, len2 * 1, 1); }
            if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.apply_spent(retptr, ptr0, len0, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr4 = r0;
        var len4 = r1;
        if (r3) {
            ptr4 = 0; len4 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred5_0 = ptr4;
        deferred5_1 = len4;
        return getStringFromWasm0(ptr4, len4);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred5_0, deferred5_1, 1);
    }
}

/**
 * Runs a circuit's deterministic setup; returns the serialised proving key (cache it).
 * @param {string} name
 * @returns {Uint8Array}
 */
export function generate_proving_key(name) {
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(name, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        wasm.generate_proving_key(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        if (r3) {
            throw takeFromExternrefTable0(r2);
        }
        var v2 = getArrayU8FromWasm0(r0, r1).slice();
        wasm.__wbindgen_export(r0, r1 * 1, 1);
        return v2;
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
    }
}

/**
 * `{ notes: [[index, nullifier]], swaps: [[index, nullifier]] }` for everything unspent/unclaimed.
 * @param {string} wallet_json
 * @returns {string}
 */
export function nullifiers(wallet_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        wasm.nullifiers(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Deposit message for `ft_transfer_call`. Returns `{ msg, commitment }`.
 * @param {string} wallet_json
 * @param {number} asset_id
 * @param {string} amount_str
 * @param {Uint8Array | number[]} seed_bytes
 * @returns {string}
 */
export function prepare_deposit(wallet_json, asset_id, amount_str, seed_bytes) {
    let deferred6_0;
    let deferred6_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const val4 = +asset_id;
        let ptr0, len0, ptr1, len1, ptr2, len2;
        try {
            ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(amount_str, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len1 = WASM_VECTOR_LEN;
            ptr2 = passArray8ToWasm0(seed_bytes, wasm.__wbindgen_export2);
            len2 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr2) { wasm.__wbindgen_export(ptr2, len2 * 1, 1); }
            if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.prepare_deposit(retptr, ptr0, len0, val4, ptr1, len1, ptr2, len2);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr5 = r0;
        var len5 = r1;
        if (r3) {
            ptr5 = 0; len5 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred6_0 = ptr5;
        deferred6_1 = len5;
        return getStringFromWasm0(ptr5, len5);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred6_0, deferred6_1, 1);
    }
}

/**
 * Appends leaves (`[{position, cm, ciphertext}]`, base64 ciphertexts) and trial-decrypts them.
 * Returns `{ wallet, report }`.
 * @param {string} wallet_json
 * @param {string} leaves_json
 * @returns {string}
 */
export function scan(wallet_json, leaves_json) {
    let deferred4_0;
    let deferred4_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        let ptr0, len0, ptr1, len1;
        try {
            ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len0 = WASM_VECTOR_LEN;
            ptr1 = passStringToWasm0(leaves_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            len1 = WASM_VECTOR_LEN;
        } catch (e) {
            if (ptr1) { wasm.__wbindgen_export(ptr1, len1 * 1, 1); }
            if (ptr0) { wasm.__wbindgen_export(ptr0, len0 * 1, 1); }
            throw e;
        }
        wasm.scan(retptr, ptr0, len0, ptr1, len1);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr3 = r0;
        var len3 = r1;
        if (r3) {
            ptr3 = 0; len3 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred4_0 = ptr3;
        deferred4_1 = len3;
        return getStringFromWasm0(ptr3, len3);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred4_0, deferred4_1, 1);
    }
}

export function start() {
    wasm.start();
}

/**
 * @param {string} wallet_json
 * @returns {string}
 */
export function wallet_address(wallet_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        wasm.wallet_address(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred3_0, deferred3_1, 1);
    }
}

/**
 * New wallet from 32 random bytes. Returns the wallet JSON.
 * @param {Uint8Array | number[]} seed_bytes
 * @returns {string}
 */
export function wallet_new(seed_bytes) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passArray8ToWasm0(seed_bytes, wasm.__wbindgen_export2);
        const len0 = WASM_VECTOR_LEN;
        wasm.wallet_new(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred3_0, deferred3_1, 1);
    }
}

/**
 * Public view of the wallet for the UI: balances, notes and swaps without secrets.
 * @param {string} wallet_json
 * @returns {string}
 */
export function wallet_summary(wallet_json) {
    let deferred3_0;
    let deferred3_1;
    try {
        const retptr = wasm.__wbindgen_add_to_stack_pointer(-16) >>> 0;
        const ptr0 = passStringToWasm0(wallet_json, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
        const len0 = WASM_VECTOR_LEN;
        wasm.wallet_summary(retptr, ptr0, len0);
        var r0 = getDataViewMemory0().getInt32(retptr + 4 * 0, true);
        var r1 = getDataViewMemory0().getInt32(retptr + 4 * 1, true);
        var r2 = getDataViewMemory0().getInt32(retptr + 4 * 2, true);
        var r3 = getDataViewMemory0().getInt32(retptr + 4 * 3, true);
        var ptr2 = r0;
        var len2 = r1;
        if (r3) {
            ptr2 = 0; len2 = 0;
            throw takeFromExternrefTable0(r2);
        }
        deferred3_0 = ptr2;
        deferred3_1 = len2;
        return getStringFromWasm0(ptr2, len2);
    } finally {
        wasm.__wbindgen_add_to_stack_pointer(16);
        wasm.__wbindgen_export(deferred3_0, deferred3_1, 1);
    }
}
function __wbg_get_imports() {
    const import0 = {
        __proto__: null,
        __wbg_Error_208446807d0d1e30: function(arg0, arg1) {
            const ret = Error(getStringFromWasm0(arg0, arg1));
            return ret;
        },
        __wbg___wbindgen_throw_4c88d8bf264fd984: function(arg0, arg1) {
            throw new Error(getStringFromWasm0(arg0, arg1));
        },
        __wbg_error_757e9472f8410341: function(arg0, arg1) {
            let deferred0_0;
            let deferred0_1;
            try {
                deferred0_0 = arg0;
                deferred0_1 = arg1;
                console.error(getStringFromWasm0(arg0, arg1));
            } finally {
                wasm.__wbindgen_export(deferred0_0, deferred0_1, 1);
            }
        },
        __wbg_new_bb74001155cb8ca8: function() {
            const ret = new Error();
            return ret;
        },
        __wbg_stack_575c4e0918581b9c: function(arg0, arg1) {
            const ret = arg1.stack;
            const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_export2, wasm.__wbindgen_export3);
            const len1 = WASM_VECTOR_LEN;
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 1, len1, true);
            getDataViewMemory0().setInt32((arg0 >>> 0) + 4 * 0, ptr1, true);
        },
        __wbindgen_init_externref_table: function() {
            const table = wasm.__wbindgen_externrefs;
            table.set(0, undefined);
            table.set(1024, undefined);
            table.set(1025, null);
            table.set(1026, true);
            table.set(1027, false);
        },
    };
    return {
        __proto__: null,
        "./antumbra_wasm_bg.js": import0,
    };
}

const ProverFinalization = (typeof FinalizationRegistry === 'undefined')
    ? { register: () => {}, unregister: () => {} }
    : new FinalizationRegistry(ptr => wasm.__wbg_prover_free(ptr, 1));

function getArrayU8FromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}

let cachedDataViewMemory0 = null;
function getDataViewMemory0() {
    if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && cachedDataViewMemory0.buffer !== wasm.memory.buffer)) {
        cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
    }
    return cachedDataViewMemory0;
}

function getStringFromWasm0(ptr, len) {
    ptr = ptr >>> 0;
    return decodeText(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}

let cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
    if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
        cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
    }
    return cachedUint8ArrayMemory0;
}

function passArray8ToWasm0(arg, malloc) {
    const ptr = malloc(arg.length * 1, 1) >>> 0;
    getUint8ArrayMemory0().set(arg, ptr / 1);
    WASM_VECTOR_LEN = arg.length;
    return ptr;
}

function passStringToWasm0(arg, malloc, realloc) {
    if (realloc === undefined) {
        const buf = cachedTextEncoder.encode(arg);
        const ptr = malloc(buf.length, 1) >>> 0;
        getUint8ArrayMemory0().subarray(ptr, ptr + buf.length).set(buf);
        WASM_VECTOR_LEN = buf.length;
        return ptr;
    }

    let len = arg.length;
    let ptr = malloc(len, 1) >>> 0;

    const mem = getUint8ArrayMemory0();

    let offset = 0;

    for (; offset < len; offset++) {
        const code = arg.charCodeAt(offset);
        if (code > 0x7F) break;
        mem[ptr + offset] = code;
    }
    if (offset !== len) {
        if (offset !== 0) {
            arg = arg.slice(offset);
        }
        ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
        const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
        const ret = cachedTextEncoder.encodeInto(arg, view);

        offset += ret.written;
        ptr = realloc(ptr, len, offset, 1) >>> 0;
    }

    WASM_VECTOR_LEN = offset;
    return ptr;
}

function takeFromExternrefTable0(idx) {
    const value = wasm.__wbindgen_externrefs.get(idx);
    wasm.__wbindgen_export4(idx);
    return value;
}

let cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
cachedTextDecoder.decode();
const MAX_SAFARI_DECODE_BYTES = 2146435072;
let numBytesDecoded = 0;
function decodeText(bytes) {
    numBytesDecoded += bytes.length;
    if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
        cachedTextDecoder = new TextDecoder('utf-8', { ignoreBOM: true });
        cachedTextDecoder.decode();
        numBytesDecoded = bytes.length;
    }
    return cachedTextDecoder.decode(bytes);
}

const cachedTextEncoder = new TextEncoder();

if (!('encodeInto' in cachedTextEncoder)) {
    cachedTextEncoder.encodeInto = function (arg, view) {
        const buf = cachedTextEncoder.encode(arg);
        view.set(buf);
        return {
            read: arg.length,
            written: buf.length
        };
    };
}

let WASM_VECTOR_LEN = 0;

let wasmModule, wasmInstance, wasm;
function __wbg_finalize_init(instance, module) {
    wasmInstance = instance;
    wasm = instance.exports;
    wasmModule = module;
    cachedDataViewMemory0 = null;
    cachedUint8ArrayMemory0 = null;
    wasm.__wbindgen_start();
    return wasm;
}

async function __wbg_load(module, imports) {
    if (typeof Response === 'function' && module instanceof Response) {
        if (!module.ok) {
            throw new Error(`failed to fetch Wasm: ${module.status} ${module.statusText} fetching '${module.url}'`);
        }

        if (typeof WebAssembly.instantiateStreaming === 'function') {
            try {
                return await WebAssembly.instantiateStreaming(module, imports);
            } catch (e) {
                const validResponse = expectedResponseType(module.type);

                if (validResponse && module.headers.get('Content-Type') !== 'application/wasm') {
                    console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);

                } else { throw e; }
            }
        }

        const bytes = await module.arrayBuffer();
        return await WebAssembly.instantiate(bytes, imports);
    } else {
        const instance = await WebAssembly.instantiate(module, imports);

        if (instance instanceof WebAssembly.Instance) {
            return { instance, module };
        } else {
            return instance;
        }
    }

    function expectedResponseType(type) {
        switch (type) {
            case 'basic': case 'cors': case 'default': return true;
        }
        return false;
    }
}

function initSync(module) {
    if (wasm !== undefined) return wasm;


    if (module !== undefined) {
        if (Object.getPrototypeOf(module) === Object.prototype) {
            ({module} = module)
        } else {
            console.warn('using deprecated parameters for `initSync()`; pass a single object instead')
        }
    }

    const imports = __wbg_get_imports();
    if (!(module instanceof WebAssembly.Module)) {
        module = new WebAssembly.Module(module);
    }
    const instance = new WebAssembly.Instance(module, imports);
    return __wbg_finalize_init(instance, module);
}

let __wbg_init_promise;
async function __wbg_init(module_or_path) {
    if (wasm !== undefined) return wasm;
    if (__wbg_init_promise === undefined) {
        __wbg_init_promise = __wbg_init_once(module_or_path);
        __wbg_init_promise.catch(() => { __wbg_init_promise = undefined; });
    }
    return __wbg_init_promise;
}

async function __wbg_init_once(module_or_path) {

    if (module_or_path !== undefined) {
        if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
            ({module_or_path} = module_or_path)
        } else {
            console.warn('using deprecated parameters for the initialization function; pass a single object instead')
        }
    }

    if (module_or_path === undefined) {
        module_or_path = new URL('antumbra_wasm_bg.wasm', import.meta.url);
    }
    const imports = __wbg_get_imports();

    if (typeof module_or_path === 'string' || (typeof Request === 'function' && module_or_path instanceof Request) || (typeof URL === 'function' && module_or_path instanceof URL)) {
        module_or_path = fetch(module_or_path);
    }

    const { instance, module } = await __wbg_load(await module_or_path, imports);

    return __wbg_finalize_init(instance, module);
}

export { initSync, __wbg_init as default };
