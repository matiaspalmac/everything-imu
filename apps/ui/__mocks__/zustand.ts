// Vitest auto-mock for zustand (activated by `vi.mock("zustand")` in
// vitest.setup.ts). Wraps `create`/`createStore` so every store registers a
// reset function; after each test all stores snap back to their initial
// state. Stores are module-level singletons — without this, state leaks
// between tests and assertions become order-dependent.
// Pattern from the official zustand testing guide, minus testing-library
// (these are plain store tests, no React rendering involved).
import { afterEach, vi } from "vitest";
import type * as ZustandExportedTypes from "zustand";

export * from "zustand";

const { create: actualCreate, createStore: actualCreateStore } =
  await vi.importActual<typeof ZustandExportedTypes>("zustand");

export const storeResetFns = new Set<() => void>();

const createUncurried = <T>(stateCreator: ZustandExportedTypes.StateCreator<T>) => {
  const store = actualCreate(stateCreator);
  const initialState = store.getInitialState();
  storeResetFns.add(() => {
    store.setState(initialState, true);
  });
  return store;
};

export const create = (<T>(stateCreator: ZustandExportedTypes.StateCreator<T>) => {
  return typeof stateCreator === "function" ? createUncurried(stateCreator) : createUncurried;
}) as typeof ZustandExportedTypes.create;

const createStoreUncurried = <T>(stateCreator: ZustandExportedTypes.StateCreator<T>) => {
  const store = actualCreateStore(stateCreator);
  const initialState = store.getInitialState();
  storeResetFns.add(() => {
    store.setState(initialState, true);
  });
  return store;
};

export const createStore = (<T>(stateCreator: ZustandExportedTypes.StateCreator<T>) => {
  return typeof stateCreator === "function"
    ? createStoreUncurried(stateCreator)
    : createStoreUncurried;
}) as typeof ZustandExportedTypes.createStore;

afterEach(() => {
  for (const resetFn of storeResetFns) {
    resetFn();
  }
});
