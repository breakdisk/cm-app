package io.logisticos.driver.security

import android.content.Context
import com.scottyab.rootbeer.RootBeer
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject

class RootChecker @Inject constructor(
    @ApplicationContext private val context: Context
) {
    // Secondary constructor for testing — bypasses RootBeer entirely
    internal constructor(isRooted: Boolean) : this(context = android.app.Application()) {
        this._isRootedOverride = isRooted
    }

    private var _isRootedOverride: Boolean? = null

    fun check(): Boolean {
        _isRootedOverride?.let { return it }
        return try {
            RootBeer(context).isRooted
        } catch (e: Exception) {
            false
        }
    }
}
