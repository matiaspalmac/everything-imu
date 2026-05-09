import { Canvas, useFrame } from "@react-three/fiber";
import { useRef } from "react";
import type * as THREE from "three";

function Cuboid({ quat }: { quat: [number, number, number, number] }) {
  const ref = useRef<THREE.Mesh>(null);
  useFrame(() => {
    if (ref.current) {
      ref.current.quaternion.set(quat[0], quat[1], quat[2], quat[3]);
    }
  });
  return (
    <>
      <mesh ref={ref}>
        <boxGeometry args={[0.6, 0.2, 1.0]} />
        <meshStandardMaterial color="#0ea5e9" roughness={0.5} />
      </mesh>
      <axesHelper args={[1.5]} />
    </>
  );
}

export function TrackerViz({ quat }: { quat: [number, number, number, number] }) {
  return (
    <div className="h-32 w-32">
      <Canvas camera={{ position: [2, 2, 3], fov: 45 }}>
        <ambientLight intensity={0.6} />
        <directionalLight position={[5, 5, 5]} intensity={0.8} />
        <Cuboid quat={quat} />
      </Canvas>
    </div>
  );
}
