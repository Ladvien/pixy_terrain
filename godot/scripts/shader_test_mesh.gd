@tool
extends MeshInstance3D
## Generates a test mesh with the same vertex attributes the terrain shader expects.
## Creates a 4x4 grid of cells, each 2.0 units wide, with varying texture indices
## so you can see blending and toon lighting in action.

const GRID := 4          # 4x4 cells
const CELL := 2.0        # cell_size
const HEIGHT_VAR := 1.5  # height variation for boundary cells

# Texture index → (COLOR, CUSTOM0) encoding
# Index = row*4 + col where row = dominant channel of COLOR, col = dominant of CUSTOM0
static func _tex_colors(index: int) -> Array:
	var c0 := Color(0, 0, 0, 0)
	var c1 := Color(0, 0, 0, 0)
	var row := index / 4
	var col := index % 4
	match row:
		0: c0.r = 1.0
		1: c0.g = 1.0
		2: c0.b = 1.0
		3: c0.a = 1.0
	match col:
		0: c1.r = 1.0
		1: c1.g = 1.0
		2: c1.b = 1.0
		3: c1.a = 1.0
	return [c0, c1]

func _ready() -> void:
	_generate_mesh()

func _generate_mesh() -> void:
	var st := SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)

	# Set vertex format to match terrain chunks
	st.set_custom_format(0, SurfaceTool.CUSTOM_RGBA_FLOAT)  # CUSTOM0 = color pair 1
	st.set_custom_format(1, SurfaceTool.CUSTOM_RGBA_FLOAT)  # CUSTOM1 = grass mask
	st.set_custom_format(2, SurfaceTool.CUSTOM_RGBA_FLOAT)  # CUSTOM2 = material blend

	# Assign texture indices to cells in a pattern
	# Row 0: all texture 0 (flat, same material)
	# Row 1: texture 0 left, texture 1 right (boundary in middle)
	# Row 2: texture 4 (green group)
	# Row 3: mixed 0/1/4 (3-way boundary)
	var cell_textures := [
		[0, 0, 0, 0],
		[0, 0, 1, 1],
		[4, 4, 4, 4],
		[0, 1, 4, 0],
	]

	# Heights: mostly flat, with some variation in rows 1 and 3
	var heights := []
	for z in range(GRID + 1):
		var row := []
		for x in range(GRID + 1):
			var h := 0.0
			if z == 2 and x >= 2:
				h = HEIGHT_VAR  # step up for boundary demo
			if z >= 3:
				h = HEIGHT_VAR * 0.5
			row.append(h)
		heights.append(row)

	for cz in range(GRID):
		for cx in range(GRID):
			var tex_idx: int = cell_textures[cz][cx]
			var colors := _tex_colors(tex_idx)
			var c0: Color = colors[0]
			var c1: Color = colors[1]

			# Corner heights
			var h_a: float = heights[cz][cx]       # top-left
			var h_b: float = heights[cz][cx + 1]   # top-right
			var h_c: float = heights[cz + 1][cx]    # bottom-left
			var h_d: float = heights[cz + 1][cx + 1] # bottom-right

			# World positions
			var ax := Vector3(cx * CELL, h_a, cz * CELL)
			var bx := Vector3((cx + 1) * CELL, h_b, cz * CELL)
			var cx_ := Vector3(cx * CELL, h_c, (cz + 1) * CELL)
			var dx := Vector3((cx + 1) * CELL, h_d, (cz + 1) * CELL)

			# Check if this is a boundary cell (height varies)
			var is_boundary := not is_equal_approx(h_a, h_b) or not is_equal_approx(h_a, h_c) or not is_equal_approx(h_a, h_d)

			# CUSTOM2: material blend data
			# R = packed mat IDs (mat_a + mat_b * 16) / 255
			# G = mat_c / 15
			# B = weight_a
			# A = weight_b (>= 1.5 flags vertex-color mode for boundary cells)
			var packed_mats := (float(tex_idx) + float(tex_idx) * 16.0) / 255.0
			var custom2 := Color(packed_mats, float(tex_idx) / 15.0, 1.0, 0.0)
			if is_boundary:
				custom2.a = 2.0  # flag: use vertex color blending

			# CUSTOM1: grass mask (R=mask, G=ridge flag)
			var custom1 := Color(1.0, 0.0, 0.0, 0.0)

			# Floor triangles: A-B-D, A-D-C
			var is_floor := true
			_add_vert(st, ax, c0, c1, custom1, custom2,
				Vector2(0, 0), Vector2(ax.x, ax.z), is_floor)
			_add_vert(st, bx, c0, c1, custom1, custom2,
				Vector2(1, 0), Vector2(bx.x, bx.z), is_floor)
			_add_vert(st, dx, c0, c1, custom1, custom2,
				Vector2(1, 1), Vector2(dx.x, dx.z), is_floor)

			_add_vert(st, ax, c0, c1, custom1, custom2,
				Vector2(0, 0), Vector2(ax.x, ax.z), is_floor)
			_add_vert(st, dx, c0, c1, custom1, custom2,
				Vector2(1, 1), Vector2(dx.x, dx.z), is_floor)
			_add_vert(st, cx_, c0, c1, custom1, custom2,
				Vector2(0, 1), Vector2(cx_.x, cx_.z), is_floor)

			# If boundary, add wall quad between the step
			if is_boundary and h_b > h_a + 0.1:
				var wall_bot_l := Vector3(ax.x + CELL, h_a, ax.z)
				var wall_top_l := bx
				var wall_bot_r := Vector3(cx_.x + CELL, heights[cz + 1][cx], (cz + 1) * CELL)
				var wall_top_r := dx
				var wall_floor := false

				_add_vert(st, wall_bot_l, c0, c1, custom1, custom2,
					Vector2(0, 0), Vector2(wall_bot_l.x, wall_bot_l.z), wall_floor)
				_add_vert(st, wall_top_l, c0, c1, custom1, custom2,
					Vector2(0, 1), Vector2(wall_top_l.x, wall_top_l.z), wall_floor)
				_add_vert(st, wall_top_r, c0, c1, custom1, custom2,
					Vector2(1, 1), Vector2(wall_top_r.x, wall_top_r.z), wall_floor)

				_add_vert(st, wall_bot_l, c0, c1, custom1, custom2,
					Vector2(0, 0), Vector2(wall_bot_l.x, wall_bot_l.z), wall_floor)
				_add_vert(st, wall_top_r, c0, c1, custom1, custom2,
					Vector2(1, 1), Vector2(wall_top_r.x, wall_top_r.z), wall_floor)
				_add_vert(st, wall_bot_r, c0, c1, custom1, custom2,
					Vector2(1, 0), Vector2(wall_bot_r.x, wall_bot_r.z), wall_floor)

	st.generate_normals()
	st.index()
	mesh = st.commit()

func _add_vert(st: SurfaceTool, pos: Vector3,
		color0: Color, color1: Color,
		custom1: Color, custom2: Color,
		uv: Vector2, uv2: Vector2, is_floor: bool) -> void:
	st.set_smooth_group(0 if is_floor else 0xFFFFFFFF)
	st.set_color(color0)
	st.set_custom(0, color1)
	st.set_custom(1, custom1)
	st.set_custom(2, custom2)
	st.set_uv(uv)
	st.set_uv2(uv2)
	st.add_vertex(pos)
