@tool
extends MeshInstance3D
## Generates a demo terrain mesh with proper vertex attributes for the terrain shader.
## Includes a flat floor with a raised plateau to show both floor and wall rendering.

@export var grid_width: int = 8
@export var grid_depth: int = 8
@export var cell_size: float = 1.0
@export var plateau_height: float = 2.0
@export var regenerate: bool = false:
	set(v):
		if v:
			_generate()

func _ready() -> void:
	_generate()

func _generate() -> void:
	var st = SurfaceTool.new()
	st.begin(Mesh.PRIMITIVE_TRIANGLES)
	st.set_custom_format(0, SurfaceTool.CUSTOM_RGBA_FLOAT)
	st.set_custom_format(1, SurfaceTool.CUSTOM_RGBA_FLOAT)
	st.set_custom_format(2, SurfaceTool.CUSTOM_RGBA_FLOAT)

	var half_w = grid_width * cell_size * 0.5
	var half_d = grid_depth * cell_size * 0.5

	for z in range(grid_depth):
		for x in range(grid_width):
			var wx = x * cell_size - half_w
			var wz = z * cell_size - half_d

			# Raised plateau in the center (2x2 cells)
			var is_plateau = (x >= 3 and x <= 4 and z >= 3 and z <= 4)
			var height = plateau_height if is_plateau else 0.0

			# Pick texture based on position for variety
			var tex_idx = (x + z * 3) % 6
			var vc0 = _tex_index_to_color0(tex_idx)
			var vc1 = _tex_index_to_color1(tex_idx)

			# CUSTOM1: grass mask (R=1 for grass, G=0 no ridge)
			var custom1 = Color(1.0, 0.0, 0.0, 0.0)

			# CUSTOM2: simple material blend (mat_a in R, weight=1.0 in B)
			var mat_packed = float(tex_idx) / 255.0
			var custom2 = Color(mat_packed, 0.0, 1.0, 0.0)

			# Floor quad (two triangles)
			var uv_base = Vector2(float(x) / grid_width, float(z) / grid_depth)
			_add_floor_quad(st, wx, wz, height, cell_size, vc0, vc1, custom1, custom2, uv_base)

			# Wall faces around the plateau edges
			if is_plateau:
				# Check each neighbor - add wall if neighbor is lower
				if x == 3 or (x > 0 and not (x-1 >= 3 and x-1 <= 4 and z >= 3 and z <= 4)):
					if x == 3:
						_add_wall_quad(st, wx, wz, 0.0, plateau_height, cell_size,
							Vector3(-1, 0, 0), vc0, vc1, custom1, custom2)
				if x == 4 or (x < grid_width-1 and not (x+1 >= 3 and x+1 <= 4 and z >= 3 and z <= 4)):
					if x == 4:
						_add_wall_quad(st, wx + cell_size, wz, 0.0, plateau_height, cell_size,
							Vector3(1, 0, 0), vc0, vc1, custom1, custom2)
				if z == 3 or (z > 0 and not (x >= 3 and x <= 4 and z-1 >= 3 and z-1 <= 4)):
					if z == 3:
						_add_wall_quad(st, wx, wz, 0.0, plateau_height, cell_size,
							Vector3(0, 0, -1), vc0, vc1, custom1, custom2)
				if z == 4 or (z < grid_depth-1 and not (x >= 3 and x <= 4 and z+1 >= 3 and z+1 <= 4)):
					if z == 4:
						_add_wall_quad(st, wx, wz + cell_size, 0.0, plateau_height, cell_size,
							Vector3(0, 0, 1), vc0, vc1, custom1, custom2)

	st.generate_tangents()
	mesh = st.commit()

func _add_floor_quad(st: SurfaceTool, wx: float, wz: float, height: float,
		size: float, vc0: Color, vc1: Color, c1: Color, c2: Color, uv_base: Vector2) -> void:
	var normal = Vector3(0, 1, 0)
	var verts = [
		Vector3(wx, height, wz),
		Vector3(wx + size, height, wz),
		Vector3(wx + size, height, wz + size),
		Vector3(wx, height, wz + size),
	]
	var uvs = [
		uv_base,
		uv_base + Vector2(1.0 / grid_width, 0),
		uv_base + Vector2(1.0 / grid_width, 1.0 / grid_depth),
		uv_base + Vector2(0, 1.0 / grid_depth),
	]

	# Triangle 1: 0-1-2
	for i in [0, 1, 2]:
		st.set_normal(normal)
		st.set_color(vc0)
		st.set_custom(0, vc1)
		st.set_custom(1, c1)
		st.set_custom(2, c2)
		st.set_uv(uvs[i])
		st.set_uv2(Vector2(verts[i].x, verts[i].z))
		st.add_vertex(verts[i])

	# Triangle 2: 0-2-3
	for i in [0, 2, 3]:
		st.set_normal(normal)
		st.set_color(vc0)
		st.set_custom(0, vc1)
		st.set_custom(1, c1)
		st.set_custom(2, c2)
		st.set_uv(uvs[i])
		st.set_uv2(Vector2(verts[i].x, verts[i].z))
		st.add_vertex(verts[i])

func _add_wall_quad(st: SurfaceTool, wx: float, wz: float, bottom: float, top: float,
		size: float, normal: Vector3, vc0: Color, vc1: Color, c1: Color, c2: Color) -> void:
	# Ridge flag in CUSTOM1.g so shader renders as wall
	var wall_c1 = Color(0.0, 1.0, 0.0, 0.0)

	var verts: Array[Vector3]
	if abs(normal.x) > 0.5:
		# X-facing wall
		verts = [
			Vector3(wx, bottom, wz),
			Vector3(wx, bottom, wz + size),
			Vector3(wx, top, wz + size),
			Vector3(wx, top, wz),
		]
	else:
		# Z-facing wall
		verts = [
			Vector3(wx, bottom, wz),
			Vector3(wx + size, bottom, wz),
			Vector3(wx + size, top, wz),
			Vector3(wx, top, wz),
		]

	# Triangle 1: 0-1-2
	for i in [0, 1, 2]:
		st.set_normal(normal)
		st.set_color(vc0)
		st.set_custom(0, vc1)
		st.set_custom(1, wall_c1)
		st.set_custom(2, c2)
		st.set_uv(Vector2(0, 0))
		st.set_uv2(Vector2(verts[i].x, verts[i].y))
		st.add_vertex(verts[i])

	# Triangle 2: 0-2-3
	for i in [0, 2, 3]:
		st.set_normal(normal)
		st.set_color(vc0)
		st.set_custom(0, vc1)
		st.set_custom(1, wall_c1)
		st.set_custom(2, c2)
		st.set_uv(Vector2(0, 0))
		st.set_uv2(Vector2(verts[i].x, verts[i].y))
		st.add_vertex(verts[i])

# Encode texture index as two vertex colors (COLOR + CUSTOM0)
# index 0-15 maps to the 4x4 grid of RGBA x RGBA channels
func _tex_index_to_color0(idx: int) -> Color:
	var group = idx / 4
	match group:
		0: return Color(1, 0, 0, 0)  # R
		1: return Color(0, 1, 0, 0)  # G
		2: return Color(0, 0, 1, 0)  # B
		3: return Color(0, 0, 0, 1)  # A
	return Color(1, 0, 0, 0)

func _tex_index_to_color1(idx: int) -> Color:
	var slot = idx % 4
	match slot:
		0: return Color(1, 0, 0, 0)  # R
		1: return Color(0, 1, 0, 0)  # G
		2: return Color(0, 0, 1, 0)  # B
		3: return Color(0, 0, 0, 1)  # A
	return Color(1, 0, 0, 0)
