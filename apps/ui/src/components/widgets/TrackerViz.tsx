/* eslint-disable react/no-unknown-property -- @react-three/fiber JSX intrinsics (args/position/intensity/etc.) */
import { Canvas, useThree } from "@react-three/fiber";
import { useEffect, useRef, useState } from "react";
import type * as THREE from "three";

const TRAIL_LENGTH = 8;
const TRAIL_BASE_OPACITY = 0.18;

/**
 * Serialised WebGL mount queue. Returning to the Dashboard mounts every
 * tracker card at once; creating N WebGL contexts in the same frame stalls
 * the main thread for hundreds of ms. Each viz waits for its slot so
 * contexts spin up one-by-one and the page stays interactive.
 */
let mountQueue: Promise<void> = Promise.resolve();
const MOUNT_GAP_MS = 60;

function acquireMountSlot(): Promise<void> {
  const slot = mountQueue.then(() => new Promise<void>((r) => setTimeout(r, MOUNT_GAP_MS)));
  mountQueue = slot;
  return slot;
}

/** Defer truthiness until the host element is on-screen AND a mount slot
 * frees up. Off-screen cards (scrolled below the fold) never pay the
 * WebGL cost at all. */
function useLazyCanvas(hostRef: React.RefObject<HTMLDivElement | null>): boolean {
  const [ready, setReady] = useState(false);
  useEffect(() => {
    const el = hostRef.current;
    if (!el || ready) return;
    let cancelled = false;
    const io = new IntersectionObserver((entries) => {
      if (entries.some((e) => e.isIntersecting)) {
        io.disconnect();
        void acquireMountSlot().then(() => {
          if (!cancelled) setReady(true);
        });
      }
    });
    io.observe(el);
    return () => {
      cancelled = true;
      io.disconnect();
    };
  }, [hostRef, ready]);
  return ready;
}

function TrailGhost({
  quat,
  index,
  total,
}: {
  quat: [number, number, number, number];
  index: number;
  total: number;
}) {
  const ref = useRef<THREE.Mesh>(null);
  useEffect(() => {
    if (ref.current) {
      ref.current.quaternion.set(quat[0], quat[1], quat[2], quat[3]);
    }
  }, [quat]);
  // Older ghosts fade out and shrink slightly so the "live" cuboid stays
  // legible while the trail visualises recent motion as a fingerprint.
  const fade = 1 - index / total;
  const scale = 0.92 + fade * 0.08;
  const opacity = TRAIL_BASE_OPACITY * fade * fade;
  return (
    <mesh ref={ref} scale={[scale, scale, scale]}>
      <boxGeometry args={[0.6, 0.2, 1.0]} />
      <meshBasicMaterial color="#ff4f0a" transparent opacity={opacity} />
    </mesh>
  );
}

/**
 * Demand-mode frame driver: the Canvas only repaints when the quaternion
 * actually changes. With the default `always` frameloop every TrackerViz
 * instance burns a 60 fps render loop even while the device sits still —
 * multiplied by one canvas per tracker card that dominated CPU usage.
 */
function InvalidateOnQuat({ quat }: { quat: [number, number, number, number] }) {
  const invalidate = useThree((s) => s.invalidate);
  // biome-ignore lint/correctness/useExhaustiveDependencies: quat is intentionally a dependency — each new pose must schedule exactly one demand-mode frame.
  useEffect(() => {
    invalidate();
  }, [quat, invalidate]);
  return null;
}

function Cuboid({
  quat,
  trail,
}: {
  quat: [number, number, number, number];
  trail: Array<[number, number, number, number]>;
}) {
  const ref = useRef<THREE.Mesh>(null);
  useEffect(() => {
    if (ref.current) {
      ref.current.quaternion.set(quat[0], quat[1], quat[2], quat[3]);
    }
  }, [quat]);
  return (
    <>
      {/*
        Spatial frame: subtle grid floor + axes helper. Grid sits below
        the cuboid; neutral charcoal tones blend with the panel's
        OKLCH palette so it reads as scene scaffolding, not chrome.
      */}
      <gridHelper args={[3, 6, "#2a2a31", "#1e1e23"]} position={[0, -0.6, 0]} />
      {trail.map((q, i) => {
        const stableKey = `ghost-${q[0].toFixed(3)}-${q[1].toFixed(3)}-${q[2].toFixed(3)}-${q[3].toFixed(3)}-${trail.length - i}`;
        return <TrailGhost key={stableKey} quat={q} index={i + 1} total={trail.length + 1} />;
      })}
      <mesh ref={ref}>
        <boxGeometry args={[0.6, 0.2, 1.0]} />
        <meshStandardMaterial color="#ff4f0a" roughness={0.45} metalness={0.1} />
      </mesh>
      <axesHelper args={[1.5]} />
    </>
  );
}

/**
 * Live cuboid + ghosted trail of the last N quaternion samples — visual
 * motion fingerprint. The trail is held in a module-scoped ring buffer
 * keyed by component instance via a tiny WeakMap so each TrackerViz on
 * the Dashboard keeps its own history without coupling to a global
 * store (we already poll IMU at 30 Hz; an additional store layer would
 * just be ceremony for what's a render-local concern).
 */
const trails = new WeakMap<object, Array<[number, number, number, number]>>();

export function TrackerViz({ quat }: { quat: [number, number, number, number] }) {
  const key = useRef({});
  const hostRef = useRef<HTMLDivElement>(null);
  const canvasReady = useLazyCanvas(hostRef);
  const history = trails.get(key.current) ?? [];
  // Push new quat unless it's effectively identical to the head (avoid
  // building a trail when the device sits still — keeps the viz clean).
  const head = history[0];
  const moved =
    !head ||
    Math.abs(head[0] - quat[0]) +
      Math.abs(head[1] - quat[1]) +
      Math.abs(head[2] - quat[2]) +
      Math.abs(head[3] - quat[3]) >
      0.002;
  if (moved) {
    history.unshift(quat);
    if (history.length > TRAIL_LENGTH) history.length = TRAIL_LENGTH;
    trails.set(key.current, history);
  }
  return (
    <div ref={hostRef} className="relative size-32">
      {canvasReady ? (
        <Canvas camera={{ position: [2, 2, 3], fov: 45 }} frameloop="demand" dpr={[1, 1.5]}>
          <InvalidateOnQuat quat={quat} />
          {/* Neutral key from one side + dim fill from opposite — sells
              the material as physical without tinting the accent hue. */}
          <ambientLight intensity={0.45} />
          <directionalLight position={[5, 5, 5]} intensity={0.8} color="#f2f2f4" />
          <directionalLight position={[-3, -2, -4]} intensity={0.25} color="#9a9aa2" />
          <Cuboid quat={quat} trail={history.slice(1)} />
        </Canvas>
      ) : (
        <div
          aria-hidden
          className="size-full rounded-[var(--radius-sm)] bg-[var(--bg-elevated)]/50"
        />
      )}
      {/* Static axis labels overlaid on the canvas corners — cheaper than
          rendering Text geometry in three.js and stays sharp at any zoom. */}
      <span className="pointer-events-none absolute right-1 top-1 text-[8px] font-bold text-[var(--danger)]">
        X
      </span>
      <span className="pointer-events-none absolute left-1 top-1 text-[8px] font-bold text-[var(--success)]">
        Y
      </span>
      <span className="pointer-events-none absolute bottom-1 right-1 text-[8px] font-bold text-[var(--info)]">
        Z
      </span>
    </div>
  );
}
