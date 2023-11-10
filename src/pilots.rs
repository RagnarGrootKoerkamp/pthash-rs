use super::*;
use bitvec::slice::BitSlice;

impl<F: Packed, Hx: Hasher> PtrHash<F, Hx> {
    pub(super) fn find_pilot(
        &self,
        kmax: u64,
        bucket: &[Hash],
        taken: &mut BitSlice,
    ) -> Option<(u64, Hash)> {
        // This gives ~10% speedup.
        match bucket.len() {
            1 => self.find_pilot_array(kmax, bucket.split_array_ref::<1>().0, taken),
            2 => self.find_pilot_array(kmax, bucket.split_array_ref::<2>().0, taken),
            3 => self.find_pilot_array(kmax, bucket.split_array_ref::<3>().0, taken),
            4 => self.find_pilot_array(kmax, bucket.split_array_ref::<4>().0, taken),
            5 => self.find_pilot_array(kmax, bucket.split_array_ref::<5>().0, taken),
            6 => self.find_pilot_array(kmax, bucket.split_array_ref::<6>().0, taken),
            7 => self.find_pilot_array(kmax, bucket.split_array_ref::<7>().0, taken),
            8 => self.find_pilot_array(kmax, bucket.split_array_ref::<8>().0, taken),
            _ => self.find_pilot_slice(kmax, bucket, taken),
        }
    }
    fn find_pilot_array<const L: usize>(
        &self,
        kmax: u64,
        bucket: &[Hash; L],
        taken: &mut BitSlice,
    ) -> Option<(u64, Hash)> {
        self.find_pilot_slice(kmax, bucket, taken)
    }

    // Note: Prefetching on `taken` is not needed because we use parts that fit in L1 cache anyway.
    //
    // Note: Tried looping over multiple pilots in parallel, but the additional
    // lookups this does aren't worth it.
    #[inline(always)]
    fn find_pilot_slice(
        &self,
        kmax: u64,
        bucket: &[Hash],
        taken: &mut BitSlice,
    ) -> Option<(u64, Hash)> {
        'p: for p in 0u64..kmax {
            let hp = self.hash_pilot(p);
            // True when the slot for hx is already taken.
            let check = |hx| unsafe { *taken.get_unchecked(self.slot_in_part_hp(hx, hp)) };

            // Process chunks of 4 bucket elements at a time.
            // This reduces branch-misses (of all of displace) 3-fold, giving 20% speedup.
            let chunks = bucket.array_chunks::<4>();
            for &hxs in chunks.clone() {
                // Check all 4 elements of the chunk without early break.
                // (Note that [_; 4]::map is non-lazy.)
                // NOTE: It's hard to SIMD vectorize the `slot` computation
                // here because it uses 64x64->128bit multiplies.
                if hxs.map(check).iter().any(|&bad| bad) {
                    continue 'p;
                }
            }
            // Check remaining elements.
            let mut bad = false;
            for &hx in chunks.remainder() {
                bad |= check(hx);
            }
            if bad {
                continue 'p;
            }

            if self.try_take_pilot(bucket, hp, taken) {
                return Some((p, hp));
            }
        }
        None
    }

    /// Fill `taken` with the slots for `hp`, but backtrack as soon as a
    /// collision within the bucket is found.
    ///
    /// Returns true on success.
    fn try_take_pilot(&self, bucket: &[Hash], hp: Hash, taken: &mut BitSlice) -> bool {
        // This bucket does not collide with previous buckets, but it may still collide with itself.
        for (i, &hx) in bucket.iter().enumerate() {
            let slot = self.slot_in_part_hp(hx, hp);
            if unsafe { *taken.get_unchecked(slot) } {
                // Collision within the bucket. Clean already set entries.
                for &hx in unsafe { bucket.get_unchecked(..i) } {
                    unsafe { taken.set_unchecked(self.slot_in_part_hp(hx, hp), false) };
                }
                return false;
            }
            unsafe { taken.set_unchecked(slot, true) };
        }
        true
    }
}
