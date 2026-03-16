/** Color palette for REXC tag characters. */
export const TAG_COLORS: Record<string, string> = {
	"'": '#fb7676',   // ref            0°
	',': '#fbb676',   // string        40°
	'+': '#b6fb76',   // integer       80°
	'*': '#76fb76',   // decimal      120°
	'key': '#76fbb6', // property key 160°
	'.': '#76b6fb',   // chain        200°
	':': '#7676fb',   // object       240°
	'^': '#b676fb',   // pointer      280°
	';': '#fb76b6',   // array        320°
	'#': '#9c9c9c',   // index
}

/** Color for b64 digits. */
export const B64_COLOR = '#888'

/** Color for annotations/comments. */
export const DIM_COLOR = '#555'
