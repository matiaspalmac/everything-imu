import { Canvas, useFrame } from "@react-three/fiber";
import { useRef } from "react";
import type * as THREE from "three";

const TRAIL_LENGTH = 8;
const TRAIL_BASE_OPACITY = 0.18;

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
  useFrame(() => {
    if (ref.current) {
      ref.current.quaternion.set(quat[0], quat[1], quat[2], quat[3]);
    }
  });
  // Older ghosts fade out and shrink slightly so the "live" cuboid stays
  // legible while the trail visualises recent motion as a fingerprint.
  const fade = 1 - index / total;
  const scale = 0.92 + fade * 0.08;
  const opacity = TRAIL_BASE_OPACITY * fade * fade;
  return (
    <mesh ref={ref} scale={[scale, scale, scale]}>
      <boxGeometry args={[0.6, 0.2, 1.0]} />
      <meshBasicMaterial color="#7aa6d6" transparent opacity={opacity} />
    </mesh>
  );
}

function Cuboid({
  quat,
  trail,
}: {
  quat: [number, number, number, number];
  trail: Array<[number, number, number, number]>;
}) {
  const ref = useRef<THREE.Mesh>(null);
  useFrame(() => {
    if (ref.current) {
      ref.current.quaternion.set(quat[0], quat[1], quat[2], quat[3]);
    }
  });
  return (
    <>
      {/*
        Spatial frame: subtle grid floor + axes helper. Grid sits below
        the cuboid; the soft accent color blends with the panel's
        OKLCH palette so it reads as scene scaffolding, not chrome.
      */}
      <gridHelper args={[3, 6, "#2a3340", "#1e2530"]} position={[0, -0.6, 0]} />
      {trail.map((q, i) => {
        const stableKey = `ghost-${q[0].toFixed(3)}-${q[1].toFixed(3)}-${q[2].toFixed(3)}-${q[3].toFixed(3)}-${trail.length - i}`;
        return <TrailGhost key={stableKey} quat={q} index={i + 1} total={trail.length + 1} />;
      })}
      <mesh ref={ref}>
        <boxGeometry args={[0.6, 0.2, 1.0]} />
        <meshStandardMaterial color="#7aa6d6" roughness={0.45} metalness={0.1} />
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
    <div className="relative h-32 w-32">
      <Canvas camera={{ position: [2, 2, 3], fov: 45 }}>
        {/* Soft warm fill from one side + cool rim from opposite — sells
            the material as physical without overpowering the axes. */}
        <ambientLight intensity={0.45} />
        <directionalLight position={[5, 5, 5]} intensity={0.8} color="#dde6f4" />
        <directionalLight position={[-3, -2, -4]} intensity={0.25} color="#a8c4e8" />
        <Cuboid quat={quat} trail={history.slice(1)} />
      </Canvas>
      {/* Static axis labels overlaid on the canvas corners — cheaper than
          rendering Text geometry in three.js and stays sharp at any zoom. */}
      <span className="pointer-events-none absolute right-1 top-1 text-[8px] font-bold text-[#e57373]">
        X
      </span>
      <span className="pointer-events-none absolute left-1 top-1 text-[8px] font-bold text-[#81c784]">
        Y
      </span>
      <span className="pointer-events-none absolute bottom-1 right-1 text-[8px] font-bold text-[#64b5f6]">
        Z
      </span>
    </div>
  );
}
