extends CharacterBody3D

@export var move_speed: float = 5.0
@export var sprint_multiplier: float = 2.0
@export var jump_velocity: float = 4.5
@export var camera_pitch: float = -45.0
@export var camera_distance: float = 12.0
@export var camera_follow_offset: Vector3 = Vector3(0, 1, 0)
@export var rotation_speed: float = 8.0
@export var rotation_step: float = 90.0
@export var player_rotation_speed: float = 10.0

var gravity: float = 9.8
var _target_yaw: float = 0.0
var _current_yaw: float = 0.0
var _camera: Camera3D

func _ready() -> void:
	add_to_group("pixy_characters")
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed:
		match event.keycode:
			KEY_ESCAPE:
				if Input.mouse_mode == Input.MOUSE_MODE_CAPTURED:
					Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
				else:
					Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
			KEY_Q:
				_target_yaw += rotation_step
			KEY_E:
				_target_yaw -= rotation_step

func _process(delta: float) -> void:
	# Lazy-get camera
	if not _camera:
		_camera = get_viewport().get_camera_3d()
	if not _camera:
		return

	# Interpolate yaw
	_current_yaw = rad_to_deg(lerp_angle(
		deg_to_rad(_current_yaw),
		deg_to_rad(_target_yaw),
		clampf(rotation_speed * delta, 0.0, 1.0)
	))

	# Update camera position
	var follow_target := global_position + camera_follow_offset
	var pitch_rad := deg_to_rad(camera_pitch)
	var yaw_rad := deg_to_rad(_current_yaw)

	var offset := Vector3(0, 0, camera_distance)
	# Rotate by pitch around X
	offset = Vector3(
		offset.x,
		offset.y * cos(pitch_rad) - offset.z * sin(pitch_rad),
		offset.y * sin(pitch_rad) + offset.z * cos(pitch_rad)
	)
	# Rotate by yaw around Y
	offset = Vector3(
		offset.x * cos(yaw_rad) + offset.z * sin(yaw_rad),
		offset.y,
		-offset.x * sin(yaw_rad) + offset.z * cos(yaw_rad)
	)

	_camera.global_position = follow_target + offset
	_camera.look_at(follow_target)

func _physics_process(delta: float) -> void:
	# Gravity
	if not is_on_floor():
		velocity.y -= gravity * delta

	# Jump
	if Input.is_key_pressed(KEY_SPACE) and is_on_floor():
		velocity.y = jump_velocity

	# Movement direction
	var input_dir := Vector2.ZERO
	if Input.is_key_pressed(KEY_W):
		input_dir.y -= 1
	if Input.is_key_pressed(KEY_S):
		input_dir.y += 1
	if Input.is_key_pressed(KEY_A):
		input_dir.x -= 1
	if Input.is_key_pressed(KEY_D):
		input_dir.x += 1
	input_dir = input_dir.normalized()

	# Rotate input by camera yaw so W = forward from camera's perspective
	var yaw_rad := deg_to_rad(_current_yaw)
	var direction := Vector3.ZERO
	if input_dir != Vector2.ZERO:
		direction = Vector3(
			input_dir.x * cos(yaw_rad) + input_dir.y * sin(yaw_rad),
			0,
			-input_dir.x * sin(yaw_rad) + input_dir.y * cos(yaw_rad)
		).normalized()

	var speed := move_speed
	if Input.is_key_pressed(KEY_SHIFT):
		speed *= sprint_multiplier

	if direction:
		velocity.x = direction.x * speed
		velocity.z = direction.z * speed
		# Rotate player model to face movement direction
		var target_angle := atan2(direction.x, direction.z)
		rotation.y = lerp_angle(rotation.y, target_angle, clampf(player_rotation_speed * delta, 0.0, 1.0))
	else:
		velocity.x = move_toward(velocity.x, 0, speed)
		velocity.z = move_toward(velocity.z, 0, speed)

	move_and_slide()
