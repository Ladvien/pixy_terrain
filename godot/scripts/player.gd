extends CharacterBody3D

@export var move_speed: float = 5.0
@export var sprint_multiplier: float = 2.0
@export var mouse_sensitivity: float = 0.002
@export var jump_velocity: float = 4.5

@onready var camera_pivot: Node3D = $CameraPivot
@onready var spring_arm: SpringArm3D = $CameraPivot/SpringArm3D
@onready var camera: Camera3D = $CameraPivot/SpringArm3D/Camera3D

var gravity: float = 9.8
var _mouse_captured: bool = true
var _third_person: bool = true
var _camera_pitch: float = 0.0

func _ready() -> void:
	add_to_group("pixy_characters")
	Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
	spring_arm.spring_length = 4.0

func _unhandled_input(event: InputEvent) -> void:
	if event is InputEventMouseMotion and _mouse_captured:
		# Yaw: rotate the whole player body
		rotate_y(-event.relative.x * mouse_sensitivity)
		# Pitch: rotate the camera pivot
		_camera_pitch -= event.relative.y * mouse_sensitivity
		_camera_pitch = clamp(_camera_pitch, deg_to_rad(-89), deg_to_rad(89))
		camera_pivot.rotation.x = _camera_pitch

	if event is InputEventKey and event.pressed:
		match event.keycode:
			KEY_ESCAPE:
				_mouse_captured = !_mouse_captured
				if _mouse_captured:
					Input.mouse_mode = Input.MOUSE_MODE_CAPTURED
				else:
					Input.mouse_mode = Input.MOUSE_MODE_VISIBLE
			KEY_V:
				_third_person = !_third_person
				if _third_person:
					spring_arm.spring_length = 4.0
				else:
					spring_arm.spring_length = 0.0

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

	var direction := (transform.basis * Vector3(input_dir.x, 0, input_dir.y)).normalized()

	var speed := move_speed
	if Input.is_key_pressed(KEY_SHIFT):
		speed *= sprint_multiplier

	if direction:
		velocity.x = direction.x * speed
		velocity.z = direction.z * speed
	else:
		velocity.x = move_toward(velocity.x, 0, speed)
		velocity.z = move_toward(velocity.z, 0, speed)

	move_and_slide()
