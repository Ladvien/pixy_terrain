@tool
extends Camera3D

var _orbit_distance := 20.0
var _orbit_yaw := 0.0
var _orbit_pitch := 30.0
var _orbit_target := Vector3.ZERO
var _dragging := false

func _ready() -> void:
	_update_transform()

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseButton:
		match event.button_index:
			MOUSE_BUTTON_RIGHT:
				_dragging = event.pressed
			MOUSE_BUTTON_WHEEL_UP:
				_orbit_distance = max(2.0, _orbit_distance - 1.0)
				_update_transform()
			MOUSE_BUTTON_WHEEL_DOWN:
				_orbit_distance = min(50.0, _orbit_distance + 1.0)
				_update_transform()
	elif event is InputEventMouseMotion and _dragging:
		_orbit_yaw -= event.relative.x * 0.3
		_orbit_pitch = clamp(_orbit_pitch - event.relative.y * 0.3, -89, 89)
		_update_transform()

func _update_transform() -> void:
	var yaw_rad = deg_to_rad(_orbit_yaw)
	var pitch_rad = deg_to_rad(_orbit_pitch)
	var offset = Vector3(
		cos(pitch_rad) * sin(yaw_rad),
		sin(pitch_rad),
		cos(pitch_rad) * cos(yaw_rad)
	) * _orbit_distance
	global_position = _orbit_target + offset
	look_at(_orbit_target)
