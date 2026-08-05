package io.logisticos.driver.core.database.dao

import android.content.Context
import androidx.room.Room
import androidx.test.core.app.ApplicationProvider
import io.logisticos.driver.core.database.DriverDatabase
import io.logisticos.driver.core.database.entity.SyncAction
import io.logisticos.driver.core.database.entity.SyncQueueEntity
import androidx.test.ext.junit.runners.AndroidJUnit4
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.test.runTest
import org.junit.After
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith
import org.junit.Assert.*

/**
 * Instrumented test: exercises Room-generated SQL against a real SQLite engine.
 *
 * Lives in androidTest, not test. It previously sat in the JVM unit-test source
 * set using `ApplicationProvider` under JUnit 5, with neither the
 * androidx.test:core dependency nor a Robolectric runner — so it could not
 * compile, and because unit tests were never executed in CI, nothing surfaced
 * that. Its compile failure was blocking the whole core:database test source set.
 *
 * Note this does not run in CI today: the driver workflow has no emulator step.
 * Run locally with `./gradlew :core:database:connectedDebugAndroidTest`.
 */
@RunWith(AndroidJUnit4::class)
class SyncQueueDaoTest {
    private lateinit var db: DriverDatabase
    private lateinit var dao: SyncQueueDao

    @Before fun setUp() {
        val context = ApplicationProvider.getApplicationContext<Context>()
        db = Room.inMemoryDatabaseBuilder(context, DriverDatabase::class.java)
            .allowMainThreadQueries()
            .build()
        dao = db.syncQueueDao()
    }

    @After fun tearDown() { db.close() }

    @Test
    fun `enqueue returns id and item is retrievable`() = runTest {
        val id = dao.enqueue(SyncQueueEntity(
            action = SyncAction.TASK_STATUS_UPDATE,
            payloadJson = """{"taskId":"t1","status":"COMPLETED"}""",
            createdAt = 1000L
        ))
        assertTrue(id > 0)
        val pending = dao.getPendingItems(now = 1000L)
        assertEquals(1, pending.size)
        assertEquals(SyncAction.TASK_STATUS_UPDATE, pending[0].action)
    }

    @Test
    fun `getPendingItems excludes future retries`() = runTest {
        dao.enqueue(SyncQueueEntity(
            action = SyncAction.POD_SUBMIT,
            payloadJson = "{}",
            createdAt = 1000L,
            nextRetryAt = 9999999L // far future
        ))
        val pending = dao.getPendingItems(now = 1000L)
        assertTrue(pending.isEmpty())
    }

    @Test
    fun `remove deletes item from queue`() = runTest {
        val id = dao.enqueue(SyncQueueEntity(
            action = SyncAction.SHIFT_END,
            payloadJson = "{}",
            createdAt = 1000L
        ))
        dao.remove(id)
        val pending = dao.getPendingItems(now = 1000L)
        assertTrue(pending.isEmpty())
    }

    @Test
    fun `getPendingCount flow updates on insert`() = runTest {
        assertEquals(0, dao.getPendingCount().first())
        dao.enqueue(SyncQueueEntity(action = SyncAction.POP_SUBMIT, payloadJson = "{}", createdAt = 1000L))
        assertEquals(1, dao.getPendingCount().first())
    }
}
