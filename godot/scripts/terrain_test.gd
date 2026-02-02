@tool
extends Node3D
## Test script to demonstrate PixyTerrain tile placement

@onready var terrain: PixyTerrain = $PixyTerrain

## Click this in the inspector to create test tiles
@export var create_test_tiles: bool = false:
	set(value):
		if value and terrain:
			_create_test_tiles()
		create_test_tiles = false  # Reset button


func _ready() -> void:
	# Only auto-create tiles at runtime, not in editor
	if not Engine.is_editor_hint():
		_create_test_tiles()


func _create_test_tiles() -> void:
	if not terrain:
		push_error("PixyTerrain node not found!")
		return

	# Clear any existing tiles
	terrain.clear_all()

	# Create a floor of tiles
	for x in range(-3, 4):
		for z in range(-3, 4):
			terrain.set_tile(x, 0, z, 1)  # Red tiles

	# Add some height variation with different colored tiles
	terrain.set_tile(0, 1, 0, 2)  # Green
	terrain.set_tile(1, 1, 0, 3)  # Blue
	terrain.set_tile(-1, 1, 0, 4) # Yellow
	terrain.set_tile(0, 1, 1, 5)  # Magenta
	terrain.set_tile(0, 1, -1, 6) # Cyan

	# Add a small tower
	terrain.set_tile(2, 1, 2, 7)  # Orange
	terrain.set_tile(2, 2, 2, 8)  # Purple
	terrain.set_tile(2, 3, 2, 1)  # Red

	print("PixyTerrain test: Created %d tiles" % terrain.get_tile_count())
	print("Bounds: ", terrain.get_bounds())


func _unhandled_input(event: InputEvent) -> void:
	# Only handle input at runtime
	if Engine.is_editor_hint():
		return

	if event is InputEventKey and event.pressed:
		var key := event as InputEventKey
		match key.keycode:
			KEY_SPACE:
				_create_test_tiles()
				print("Tiles recreated")
			KEY_C:
				terrain.clear_all()
				print("All tiles cleared")
			KEY_F:
				terrain.fill_region(-2, 0, -2, 2, 2, 2, 3)
				print("Filled region")
