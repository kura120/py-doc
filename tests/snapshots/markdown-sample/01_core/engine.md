# `core.engine`

Engine module.

Runs things.

## Classes

<a id="class.Base"></a>

### class `Base`

```python
class Base: ...
```

<a id="class.Engine"></a>

### class `Engine`

```python
class Engine(Base): ...
```

Types: [`Base`](../01_core/engine.md#class.Base)

The main engine.

**Examples**

```pycon
>>> Engine().speed
3
```

#### Attributes

<a id="attr.Engine.speed"></a>

##### `speed`

```python
speed: int = 3
```

#### Methods

<a id="method.Engine.__init__"></a>

##### `__init__()`

```python
def __init__(self, name: str = "x", *args, retries: int = 3, **kwargs) -> None: ...
```

Create an engine.

<a id="method.Engine.status"></a>

##### `status()`

```python
@property
def status(self) -> dict[str, int]: ...
```

*property*

Status map.

<a id="method.Engine.helper"></a>

##### `helper()`

```python
@staticmethod
def helper(a: Optional[int], b: Engine, c: int | None = None, d: os.PathLike = ".", e: Callable[[int], str] = str) -> list[str]: ...
```

Types: [`Engine`](../01_core/engine.md#class.Engine)

*static*

<a id="method.Engine.arun"></a>

##### `arun()`

```python
async def arun(self, /, x, *, y: int = 1) -> tuple[int, str]: ...
```

Run asynchronously.

