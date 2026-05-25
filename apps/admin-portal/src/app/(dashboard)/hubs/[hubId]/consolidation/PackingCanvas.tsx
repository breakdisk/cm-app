'use client';

import { useRef, useMemo } from 'react';
import { Canvas, useFrame } from '@react-three/fiber';
import { OrbitControls, GizmoHelper, GizmoViewport, Line } from '@react-three/drei';
import * as THREE from 'three';
import type { Placement, TruckSpec } from '@/lib/api/consolidation';

// Neon palette — mirrors design-system tokens.
const CYAN    = '#00E5FF';
const PURPLE  = '#A855F7';
const GREEN   = '#00FF88';
const AMBER   = '#FFAB00';
const RED     = '#FF3B5C';
const GLASS   = '#0A1628';

/** Convert algorithm coordinates (cm) → Three.js world units (m). */
function toM(cm: number): number { return cm / 100; }

// ── Truck wireframe ───────────────────────────────────────────────────────────

function TruckWireframe({ spec }: { spec: TruckSpec }) {
  const l = toM(spec.interior_length_cm);
  const w = toM(spec.interior_width_cm);
  const h = toM(spec.interior_height_cm);

  // Wireframe box centred at the geometric centre of the truck interior.
  const centre: [number, number, number] = [l / 2, h / 2, w / 2];

  return (
    <group position={centre}>
      {/* Ghost fill so the interior is subtly visible */}
      <mesh>
        <boxGeometry args={[l, h, w]} />
        <meshBasicMaterial color={GLASS} transparent opacity={0.08} side={THREE.BackSide} />
      </mesh>
      {/* Cyan edge wireframe */}
      <lineSegments>
        <edgesGeometry args={[new THREE.BoxGeometry(l, h, w)]} />
        <lineBasicMaterial color={CYAN} linewidth={1} transparent opacity={0.6} />
      </lineSegments>
    </group>
  );
}

// ── Single packed box ─────────────────────────────────────────────────────────

interface PackedBoxProps {
  placement:  Placement;
  isSelected: boolean;
  onClick:    () => void;
  index:      number;
}

// Deterministic colour cycle for individual boxes.
const BOX_COLORS = [PURPLE, GREEN, AMBER, '#3B82F6', '#EC4899', '#14B8A6'];

function PackedBox({ placement: p, isSelected, onClick, index }: PackedBoxProps) {
  const meshRef = useRef<THREE.Mesh>(null);

  // Algorithm coords: x=along truck length, y=along width, z=up.
  // Three.js: X=right, Y=up, Z=toward camera.
  // Mapping: algo-x → Three-X, algo-y → Three-Z, algo-z → Three-Y
  const l = toM(p.length_cm);
  const w = toM(p.width_cm);
  const h = toM(p.height_cm);

  const cx = toM(p.x) + l / 2;  // Three.js X
  const cy = toM(p.z) + h / 2;  // Three.js Y (up)
  const cz = toM(p.y) + w / 2;  // Three.js Z

  const baseColor = p.estimated
    ? AMBER
    : BOX_COLORS[index % BOX_COLORS.length];

  const color = isSelected ? '#FFFFFF' : baseColor;

  useFrame(() => {
    if (!meshRef.current) return;
    const scale = meshRef.current.scale;
    const target = isSelected ? 1.03 : 1.0;
    scale.setScalar(THREE.MathUtils.lerp(scale.x, target, 0.12));
  });

  return (
    <group position={[cx, cy, cz]}>
      <mesh ref={meshRef} onClick={(e) => { e.stopPropagation(); onClick(); }}>
        <boxGeometry args={[l, h, w]} />
        <meshStandardMaterial
          color={color}
          transparent
          opacity={isSelected ? 0.95 : 0.75}
          roughness={0.4}
          metalness={0.1}
        />
      </mesh>
      {/* Edge highlight */}
      <lineSegments>
        <edgesGeometry args={[new THREE.BoxGeometry(l, h, w)]} />
        <lineBasicMaterial
          color={isSelected ? '#FFFFFF' : color}
          transparent
          opacity={isSelected ? 1 : 0.5}
        />
      </lineSegments>
    </group>
  );
}

// ── Floor grid ────────────────────────────────────────────────────────────────

function FloorGrid({ spec }: { spec: TruckSpec }) {
  const l = toM(spec.interior_length_cm);
  const w = toM(spec.interior_width_cm);
  return (
    <gridHelper
      args={[Math.max(l, w), 20, CYAN, '#1A2540']}
      position={[l / 2, 0, w / 2]}
      rotation={[0, 0, 0]}
    />
  );
}

// ── Main canvas ───────────────────────────────────────────────────────────────

export interface PackingCanvasProps {
  spec:        TruckSpec;
  placements:  Placement[];
  selectedAwb: string | null;
  onSelect:    (awb: string | null) => void;
}

export default function PackingCanvas({
  spec,
  placements,
  selectedAwb,
  onSelect,
}: PackingCanvasProps) {
  const l = toM(spec.interior_length_cm);
  const h = toM(spec.interior_height_cm);
  const w = toM(spec.interior_width_cm);

  // Camera starts at a 45° isometric-ish angle above and to the side.
  const camPos: [number, number, number] = [l * 1.4, h * 1.8, w * 2.0];

  return (
    <Canvas
      camera={{ position: camPos, fov: 50, near: 0.01, far: 500 }}
      style={{ background: '#050810' }}
      shadows
      onClick={() => onSelect(null)}
    >
      {/* Lighting */}
      <ambientLight intensity={0.4} />
      <directionalLight position={[10, 20, 10]} intensity={0.8} castShadow />
      <pointLight position={[l / 2, h + 1, w / 2]} intensity={0.5} color={CYAN} />

      {/* Scene */}
      <FloorGrid spec={spec} />
      <TruckWireframe spec={spec} />

      {placements.map((p, i) => (
        <PackedBox
          key={p.awb}
          placement={p}
          index={i}
          isSelected={selectedAwb === p.awb}
          onClick={() => onSelect(p.awb)}
        />
      ))}

      {/* Controls */}
      <OrbitControls
        target={[l / 2, h / 4, w / 2]}
        enablePan
        enableZoom
        enableRotate
        minDistance={0.5}
        maxDistance={50}
      />
      <GizmoHelper alignment="bottom-right" margin={[80, 80]}>
        <GizmoViewport
          axisColors={[RED, GREEN, CYAN]}
          labelColor="white"
        />
      </GizmoHelper>
    </Canvas>
  );
}
