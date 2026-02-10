@tool
extends MultiMeshInstance3D
## Spawns test grass instances on the floor area for shader development.

@export var grass_count: int = 200
@export var spread: float = 3.5
@export var blade_width: float = 0.3
@export var blade_height: float = 0.6
@export var regenerate: bool = false:
	set(v):
		if v:
			_generate()

func _ready() -> void:
	_generate()

func _generate() -> void:
	var mm = MultiMesh.new()
	mm.transform_format = MultiMesh.TRANSFORM_3D
	mm.use_custom_data = true

	var quad = QuadMesh.new()
	quad.size = Vector2(blade_width, blade_height)
	quad.center_offset = Vector3(0.0, blade_height * 0.5, 0.0)
	mm.mesh = quad

	mm.instance_count = grass_count
	multimesh = mm

	var ground_colors: Array[Color] = [
		Color(0.392, 0.471, 0.318),
		Color(0.322, 0.482, 0.384),
		Color(0.373, 0.424, 0.294),
		Color(0.392, 0.475, 0.255),
		Color(0.29, 0.494, 0.365),
		Color(0.443, 0.447, 0.365),
	]

	var rng = RandomNumberGenerator.new()
	rng.seed = 42

	for i in range(grass_count):
		var x = rng.randf_range(-spread, spread)
		var z = rng.randf_range(-spread, spread)

		# Skip the plateau area (center 2x2 cells = roughly -1 to 1)
		if x > -1.0 and x < 1.0 and z > -1.0 and z < 1.0:
			# Place at edge instead
			x = rng.randf_range(1.5, spread) * (1 if rng.randf() > 0.5 else -1)

		var t = Transform3D.IDENTITY
		t.origin = Vector3(x, 0.0, z)

		var s = rng.randf_range(0.7, 1.3)
		t = t.scaled(Vector3(s, s, s))

		mm.set_instance_transform(i, t)

		var color_idx = rng.randi_range(0, 5)
		var sprite_id = float(color_idx) / 5.0
		var gc = ground_colors[color_idx]
		mm.set_instance_custom_data(i, Color(gc.r, gc.g, gc.b, sprite_id))
