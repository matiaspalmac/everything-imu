import { vi } from "vitest";

// Swap zustand for __mocks__/zustand.ts so every store auto-resets to its
// initial state after each test (see that file for details).
vi.mock("zustand");
