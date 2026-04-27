// Yaskawa Motoman-style 6-axis paint robot in side view.
// Brand orange #E8451D, with spray gun emitting paint droplets.

type Props = { size?: number };

export default function RobotIcon({ size = 36 }: Props) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 40 40"
      xmlns="http://www.w3.org/2000/svg"
      role="img"
      aria-label="Yaskawa paint robot"
      className="robot-icon"
    >
      {/* base plate */}
      <rect x="11" y="32" width="18" height="4" rx="1" fill="#374151" />
      {/* mounting flange */}
      <rect x="14" y="30" width="12" height="2" fill="#1f2937" />
      {/* swing column */}
      <rect x="16" y="22" width="8" height="9" fill="#E8451D" />
      <rect x="16" y="22" width="8" height="2" fill="#b53510" />
      {/* shoulder pivot */}
      <circle cx="20" cy="22" r="2.6" fill="#1f2937" />
      <circle cx="20" cy="22" r="0.9" fill="#9ca3af" />
      {/* upper arm */}
      <line
        x1="20"
        y1="22"
        x2="29"
        y2="13"
        stroke="#E8451D"
        strokeWidth="5"
        strokeLinecap="round"
      />
      {/* elbow */}
      <circle cx="29" cy="13" r="2.3" fill="#1f2937" />
      <circle cx="29" cy="13" r="0.8" fill="#9ca3af" />
      {/* forearm */}
      <line
        x1="29"
        y1="13"
        x2="34"
        y2="13"
        stroke="#E8451D"
        strokeWidth="4"
        strokeLinecap="round"
      />
      {/* wrist + spray gun body */}
      <rect x="34" y="11.2" width="3.2" height="4" fill="#1f2937" />
      <rect x="36.4" y="12.4" width="1.4" height="1.6" fill="#9ca3af" />
      {/* paint droplets */}
      <circle cx="38.7" cy="13.2" r="0.9" fill="#0ea5e9" />
      <circle cx="39.4" cy="11.4" r="0.55" fill="#0ea5e9" opacity="0.75" />
      <circle cx="39.4" cy="15.0" r="0.55" fill="#0ea5e9" opacity="0.75" />
      <circle cx="36.7" cy="9.5" r="0.45" fill="#0ea5e9" opacity="0.55" />
      <circle cx="36.7" cy="16.6" r="0.45" fill="#0ea5e9" opacity="0.55" />
    </svg>
  );
}
