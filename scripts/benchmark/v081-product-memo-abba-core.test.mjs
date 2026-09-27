import assert from 'node:assert/strict';
import { test } from 'node:test';
import { assertProductMemoSelection, productMemoAbbaEnvironment, productMemoAbbaSelection } from './v081-product-memo-abba-core.mjs';

test('state-major changes only product layout while both arms retain reference union storage', () => {
  assert.deepEqual(productMemoAbbaSelection('state-major', 'baseline'), { storage: 'reference', layout: 'flat', label: 'reference' });
  assert.deepEqual(productMemoAbbaSelection('state-major', 'treatment'), { storage: 'reference', layout: 'state-major', label: 'state-major' });
  assert.deepEqual(productMemoAbbaSelection('compact', 'treatment'), { storage: 'compact', layout: 'flat', label: 'compact' });
});

test('ambient product memo flags cannot contaminate a baseline or another candidate', () => {
  const env = productMemoAbbaEnvironment({ KEEP: 'yes', CLEARRA_STANDARD_BAG_MEMO: 'compact', CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT: 'state-major' }, productMemoAbbaSelection('state-major', 'baseline'));
  assert.equal(env.KEEP, 'yes');
  assert.equal(env.CLEARRA_STANDARD_BAG_MEMO, 'reference');
  assert.equal(env.CLEARRA_STANDARD_BAG_PRODUCT_MEMO_LAYOUT, 'flat');
});

test('selection must be echoed by the actual solver before its timing can be accepted', () => {
  const selected = productMemoAbbaSelection('state-major', 'treatment');
  const summary = { standard_bag_memo_storage: 'state-major', standard_bag_product_memo_storage: 'state-major', standard_bag_product_memo_layout: 'state-major', standard_bag_union_memo_storage: 'reference' };
  assert.doesNotThrow(() => assertProductMemoSelection(summary, selected));
  assert.throws(() => assertProductMemoSelection({ ...summary, standard_bag_product_memo_layout: 'flat' }, selected), /layout/);
  assert.throws(() => assertProductMemoSelection({ ...summary, standard_bag_union_memo_storage: 'compact' }, selected), /union/);
  assert.throws(() => assertProductMemoSelection({ ...summary, standard_bag_product_memo_storage: 'reference' }, selected), /storage/);
  assert.throws(() => assertProductMemoSelection({}, selected), /not applied/);
  assert.throws(() => productMemoAbbaSelection('unknown', 'baseline'));
});

test('adaptive receipts distinguish policy from actual per-worker layout and prove large-case promotion', () => {
  const selection = productMemoAbbaSelection('adaptive', 'treatment');
  assert.deepEqual(selection, { storage: 'reference', layout: 'adaptive', label: 'adaptive' });
  assert.deepEqual(productMemoAbbaSelection('adaptive', 'baseline'), { storage: 'reference', layout: 'flat', label: 'reference' });
  const small = { standard_bag_product_memo_policy: 'adaptive',
    standard_bag_product_memo_layout: 'flat', standard_bag_memo_storage: 'reference',
    standard_bag_product_memo_storage: 'reference', standard_bag_union_memo_storage: 'reference',
    standard_bag_product_memo_promotion_attempts: 0, standard_bag_product_memo_promotions: 0 };
  assert.doesNotThrow(() => assertProductMemoSelection(small, selection));
  assert.throws(() => assertProductMemoSelection(small, selection, { requirePromotion: true }), /promotion/);
  for (const layout of ['state-major', 'mixed']) {
    const large = { ...small, standard_bag_product_memo_layout: layout,
      standard_bag_product_memo_storage: layout, standard_bag_memo_storage: layout,
      standard_bag_product_memo_promotion_attempts: 2, standard_bag_product_memo_promotions: 1 };
    assert.doesNotThrow(() => assertProductMemoSelection(large, selection, { requirePromotion: true }));
    assert.throws(() => assertProductMemoSelection({ ...large, standard_bag_product_memo_promotions: 0 }, selection), /promotion/);
    assert.throws(() => assertProductMemoSelection({ ...large, standard_bag_product_memo_promotion_attempts: 0 }, selection), /promotion/);
  }
  assert.throws(() => assertProductMemoSelection({ ...small, standard_bag_product_memo_policy: undefined }, selection), /policy/);
  assert.throws(() => assertProductMemoSelection({ ...small, standard_bag_union_memo_storage: 'compact' }, selection), /union/);
  assert.throws(() => assertProductMemoSelection({ ...small, standard_bag_product_memo_promotions: 1 }, selection), /promotion/);
  for (const value of [undefined, null, true, '', ' ', NaN, Infinity, -1, 0.5]) {
    assert.throws(() => assertProductMemoSelection({ ...small, standard_bag_product_memo_promotions: value }, selection), /promotion/);
  }
});
