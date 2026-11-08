"""Things that should produce warnings.

#sd-warn: a misspelled macro
#sd-doc-link: nowhere.at_all
#sd-image: ../outside.png
#sd-code python
print("never closed")
"""


def f():
    """Function docs.

    #sd-z-index: 3
    """
